use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::rpc::Connection;

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);
// rinzed の点数づけは 1 つずつしか走らないので、打っている途中の読みの点数づけで Space での変換を待たせない
const SETTLE: Duration = Duration::from_millis(200);

// 見出しの読みが変わって SETTLE の間そのままなら、キーを送る接続とは別の接続で `lookup` を投げる。
// rinzed は最後につけた点数を覚えているので、直後に同じ読みを Space で変換すると並べ替えを待たない。
// 応答は使わず、問い合わせの途中に来たものは最新の 1 件だけ残す。rinzed が動いていなければ何もしない。
pub(crate) struct Prefetcher {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

struct Shared {
    socket: PathBuf,
    queue: Mutex<Queue>,
    wake: Condvar,
    in_flight: Mutex<Option<UnixStream>>,
}

enum Queue {
    Empty,
    Pending {
        reading: String,
        left_context: String,
    },
    Closed,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Prefetcher {
    pub(crate) fn new(socket: PathBuf) -> Self {
        let shared = Arc::new(Shared {
            socket,
            queue: Mutex::new(Queue::Empty),
            wake: Condvar::new(),
            in_flight: Mutex::new(None),
        });
        let worker = {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || shared.run())
        };
        Prefetcher {
            shared,
            worker: Some(worker),
        }
    }

    pub(crate) fn request(&self, reading: &str, left_context: &str) {
        let mut queue = lock(&self.shared.queue);
        if let Queue::Closed = *queue {
            return;
        }
        *queue = Queue::Pending {
            reading: reading.to_string(),
            left_context: left_context.to_string(),
        };
        self.shared.wake.notify_one();
    }
}

impl Drop for Prefetcher {
    fn drop(&mut self) {
        *lock(&self.shared.queue) = Queue::Closed;
        self.shared.wake.notify_one();
        if let Some(stream) = lock(&self.shared.in_flight).take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Shared {
    fn next(&self) -> Option<(String, String)> {
        let mut queue = lock(&self.queue);
        loop {
            match &*queue {
                Queue::Pending { .. } => {
                    let (guard, waited) = self
                        .wake
                        .wait_timeout(queue, SETTLE)
                        .unwrap_or_else(|e| e.into_inner());
                    queue = guard;
                    if !waited.timed_out() {
                        continue;
                    }
                    match std::mem::replace(&mut *queue, Queue::Empty) {
                        Queue::Pending {
                            reading,
                            left_context,
                        } => return Some((reading, left_context)),
                        other => *queue = other,
                    }
                }
                Queue::Closed => return None,
                Queue::Empty => queue = self.wake.wait(queue).unwrap_or_else(|e| e.into_inner()),
            }
        }
    }

    fn run(&self) {
        let mut connection: Option<Connection> = None;
        while let Some((reading, left_context)) = self.next() {
            if connection.is_none() {
                connection = Connection::open(&self.socket, LOOKUP_TIMEOUT).ok();
                *lock(&self.in_flight) = connection
                    .as_ref()
                    .and_then(|c| c.stream().try_clone().ok());
            }
            let Some(c) = connection.as_mut() else {
                continue;
            };
            if c.call(
                "lookup",
                vec![reading.into(), left_context.into()],
                LOOKUP_TIMEOUT,
            )
            .is_err()
            {
                connection = None;
            }
        }
    }
}
