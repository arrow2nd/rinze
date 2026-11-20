use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
// 起動に失敗したあと、打鍵のたびに起動し直さない
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
const STDERR_LINES_IN_REASON: usize = 4;

pub(crate) struct Launcher {
    program: Option<PathBuf>,
    socket: PathBuf,
    state: Mutex<Launch>,
}

enum Launch {
    Idle,
    Running {
        since: Instant,
        exit: Arc<Mutex<Option<Exit>>>,
    },
    Failed {
        reason: String,
        since: Instant,
    },
}

enum Exit {
    // rinzed はソケットのロックを取れなければ 0 で終了する。先に起動した rinzed の待ち受けを待つ
    AnotherServes,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Blocked {
    Starting,
    Unavailable(String),
}

impl Launcher {
    pub(crate) fn new(program: Option<PathBuf>, socket: PathBuf) -> Self {
        Launcher {
            program,
            socket,
            state: Mutex::new(Launch::Idle),
        }
    }

    fn state(&self) -> MutexGuard<'_, Launch> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn connected(&self) {
        *self.state() = Launch::Idle;
    }

    pub(crate) fn not_connected(&self) -> Blocked {
        let mut state = self.state();
        loop {
            match &*state {
                Launch::Idle => {
                    *state = self.spawn();
                    if let Launch::Running { .. } = *state {
                        return Blocked::Starting;
                    }
                }
                Launch::Running { since, exit } => {
                    let exited = exit.lock().unwrap_or_else(|e| e.into_inner()).take();
                    match exited {
                        Some(Exit::Failed(reason)) => {
                            *state = Launch::Failed {
                                reason,
                                since: Instant::now(),
                            }
                        }
                        _ if since.elapsed() > STARTUP_TIMEOUT => {
                            *state = Launch::Failed {
                                reason: format!(
                                    "rinzed が {} 秒以内に {} で待ち受けを始めなかった",
                                    STARTUP_TIMEOUT.as_secs(),
                                    self.socket.display()
                                ),
                                since: Instant::now(),
                            }
                        }
                        Some(Exit::AnotherServes) | None => return Blocked::Starting,
                    }
                }
                Launch::Failed { since, .. } if since.elapsed() >= RETRY_INTERVAL => {
                    *state = Launch::Idle;
                }
                Launch::Failed { reason, .. } => return Blocked::Unavailable(reason.clone()),
            }
        }
    }

    fn spawn(&self) -> Launch {
        let failed = |reason| Launch::Failed {
            reason,
            since: Instant::now(),
        };
        let Some(program) = &self.program else {
            return failed(format!("{} に接続できない", self.socket.display()));
        };
        let child = Command::new(program)
            .arg("--socket")
            .arg(&self.socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return failed(if program.components().count() == 1 {
                    format!("{} が PATH に無い", program.display())
                } else {
                    format!("{} が無い", program.display())
                });
            }
            Err(e) => return failed(format!("{} を起動できない: {e}", program.display())),
        };
        let exit = Arc::new(Mutex::new(None));
        let stderr = child.stderr.take().expect("stderr を piped にした");
        let program = program.clone();
        let reported = Arc::clone(&exit);
        // rinzed は変換のたびに標準エラー出力へ書くので、読み続けないとパイプが詰まって止まる。
        // 書かれる内容には読みや候補が入るので、失敗の理由に使う末尾の数行だけを持つ
        std::thread::spawn(move || {
            let tail = last_lines(stderr);
            let exit = match child.wait() {
                Ok(status) if status.success() => Exit::AnotherServes,
                Ok(status) => Exit::Failed(describe_exit(&program, &status.to_string(), &tail)),
                Err(e) => Exit::Failed(format!("{} の終了を待てない: {e}", program.display())),
            };
            *reported.lock().unwrap_or_else(|e| e.into_inner()) = Some(exit);
        });
        Launch::Running {
            since: Instant::now(),
            exit,
        }
    }
}

fn last_lines(stderr: impl std::io::Read) -> VecDeque<String> {
    let mut tail = VecDeque::new();
    let mut reader = BufReader::new(stderr);
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
        let text = String::from_utf8_lossy(&line).trim().to_string();
        line.clear();
        if text.is_empty() {
            continue;
        }
        if tail.len() == STDERR_LINES_IN_REASON {
            tail.pop_front();
        }
        tail.push_back(text);
    }
    tail
}

// rinzed は終了の理由を `rinzed: ` で始めて書く。llama.cpp もモデルの読み込みで標準エラー出力に書くので、それより rinzed の行を選ぶ。
// 引数の誤りでは理由のあとに使い方を書くので、rinzed の行が無ければ末尾の数行を並べる
fn describe_exit(program: &Path, status: &str, tail: &VecDeque<String>) -> String {
    let said = match tail.iter().rev().find(|line| line.starts_with("rinzed: ")) {
        Some(line) => line.clone(),
        None => tail
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" / "),
    };
    format!("{} が終了した ({status}): {said}", program.display())
}
