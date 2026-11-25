//! 1 本のスレッドで状態 `S` を持ち、頼まれた計算を 1 つずつ、期限つきで行う。
//!
//! 期限までに答えが返らなければ、頼んだ側は待つのをやめる。計算そのものは止められないので、
//! 期限を過ぎた計算が走っている間に来た依頼はすぐに断り、待ち行列で期限を過ぎた依頼は計算せずに捨てる。

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Job<S> = Box<dyn FnOnce(&mut S) + Send>;

pub(crate) struct Worker<S> {
    jobs: Sender<(Instant, Job<S>)>,
    /// 走っている計算の期限。何も走っていなければ None。
    running: Arc<Mutex<Option<Instant>>>,
}

impl<S: Send + 'static> Worker<S> {
    pub(crate) fn spawn(mut state: S) -> Self {
        let (jobs, queue) = mpsc::channel::<(Instant, Job<S>)>();
        let running = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&running);
        std::thread::spawn(move || {
            for (deadline, job) in queue {
                if Instant::now() > deadline {
                    continue;
                }
                *lock(&shared) = Some(deadline);
                let _ = catch_unwind(AssertUnwindSafe(|| job(&mut state)));
                *lock(&shared) = None;
            }
        });
        Self { jobs, running }
    }

    /// `job` を `timeout` までに終えればその結果を、終えなければ断った理由を返す。
    pub(crate) fn call<R: Send + 'static>(
        &self,
        timeout: Duration,
        job: impl FnOnce(&mut S) -> R + Send + 'static,
    ) -> Result<R, String> {
        let now = Instant::now();
        if lock(&self.running).is_some_and(|deadline| now > deadline) {
            return Err("期限を過ぎた前の計算がまだ終わらない".into());
        }
        let deadline = now + timeout;
        let (reply, result) = mpsc::sync_channel(1);
        self.jobs
            .send((
                deadline,
                Box::new(move |state| {
                    let _ = reply.send(job(state));
                }),
            ))
            .map_err(|_| "計算するスレッドが無い")?;
        result.recv_timeout(timeout).map_err(|e| match e {
            RecvTimeoutError::Timeout => format!("{} ms 以内に終わらない", timeout.as_millis()),
            RecvTimeoutError::Disconnected => "計算が途中で止まった".into(),
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: Duration = Duration::from_millis(100);

    #[test]
    fn 期限までに終えた計算の結果を返す() {
        let worker = Worker::spawn(1);
        assert_eq!(worker.call(TIMEOUT, |n| *n + 1), Ok(2));
    }

    #[test]
    fn 期限を過ぎた計算が終わるまでは待たずに断り_終われば引き受け直す() {
        let worker = Worker::spawn(0);
        let started = Instant::now();
        assert!(
            worker
                .call(TIMEOUT, |n| {
                    std::thread::sleep(TIMEOUT * 3);
                    *n += 1;
                })
                .is_err()
        );
        assert!(started.elapsed() < TIMEOUT * 2, "期限で待つのをやめる");

        std::thread::sleep(TIMEOUT / 2);
        let refused = Instant::now();
        assert!(worker.call(TIMEOUT, |n| *n).is_err());
        assert!(
            refused.elapsed() < TIMEOUT / 2,
            "遅れた計算の走っている間は期限まで待たずに断る"
        );

        std::thread::sleep(TIMEOUT * 2);
        assert_eq!(
            worker.call(TIMEOUT, |n| *n),
            Ok(1),
            "遅れた計算が終われば次の依頼を計算する"
        );
    }

    #[test]
    fn 待ち行列で期限を過ぎた依頼は計算しない() {
        let worker = Arc::new(Worker::spawn(Vec::new()));
        let slow = Arc::clone(&worker);
        let first = std::thread::spawn(move || {
            slow.call(TIMEOUT * 3, |log: &mut Vec<&str>| {
                std::thread::sleep(TIMEOUT * 2);
                log.push("遅い");
            })
        });
        std::thread::sleep(TIMEOUT / 4);
        assert!(
            worker
                .call(TIMEOUT, |log: &mut Vec<&str>| log.push("待った"))
                .is_err()
        );
        assert!(first.join().unwrap().is_ok());
        assert_eq!(worker.call(TIMEOUT, |log| log.clone()), Ok(vec!["遅い"]));
    }

    #[test]
    fn 計算が_panic_しても次の依頼を引き受ける() {
        let worker = Worker::spawn(1);
        assert!(
            worker
                .call(TIMEOUT, |_| -> i32 { panic!("壊れた") })
                .is_err()
        );
        assert_eq!(worker.call(TIMEOUT, |n| *n), Ok(1));
    }
}
