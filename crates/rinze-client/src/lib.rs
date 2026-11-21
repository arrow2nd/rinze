//! rinzed と msgpack-rpc で話すクライアント。fcitx5 などのフロントエンドから C ABI ([`ffi`]) で使う。
//!
//! [`Service`] は 1 つの rinzed (ソケット) に当たり、起動を受け持つ。フロントエンドに 1 つ作る。
//! [`Client`] は rinzed の 1 つの入力状態 (1 接続) に当たり、入力欄ごとに 1 つ作る。

mod launch;
mod left_context;
mod response;
mod rpc;

pub mod ffi;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rmpv::Value;

pub use left_context::{LeftContext, Origin, Surrounding, left_context};

use left_context::PreeditTrail;
pub use response::{CandidatePage, Minibuffer, Mode, Response};

use launch::{Blocked, Launcher};
use rpc::{CallError, Connection};

// 待つ間は fcitx5 全体が固まる。CPU が詰まっていても変換を待てて、止まった rinzed からはいずれ抜けられる長さにする
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
// 一度待ち切れなかったあとは、応答が来るまで短く待つ。止まった rinzed に打鍵のたびに長く固まらない
const STALLED_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_SWITCH_KEYS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// rinzed を起動した (または起動中の) ので、この要求は処理していない。
    Starting,
    /// 接続できず、起動もできなかった。
    Unavailable(String),
    /// 時間内に応答が無かったが、要求は rinzed に届いていて入力状態も残っている。
    /// 表示はそのままにする。応答は次の要求の前に読み、その確定文字列は次の応答の `commit` に含める。
    Delayed(String),
    /// 接続が切れたか、応答を待ち切れずに諦めた。rinzed 側の入力状態は失われた。
    Lost(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Starting => write!(f, "rinzed を起動しています"),
            Error::Unavailable(reason) | Error::Delayed(reason) | Error::Lost(reason) => {
                write!(f, "{reason}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Ready,
    Starting,
    Unavailable(String),
}

pub struct Service {
    socket: PathBuf,
    launcher: Launcher,
    // どれかの接続で応答を待ち切れず、そのあとまだどの応答も受け取っていない
    stalled: AtomicBool,
}

pub struct Options {
    /// `None` なら既定のソケット。
    pub socket: Option<PathBuf>,
    /// 接続できなかったときに `--socket` を付けて起動する rinzed。`None` なら起動しない。
    pub program: Option<PathBuf>,
}

impl Service {
    pub fn new(options: Options) -> Result<Arc<Self>, String> {
        let socket = match options.socket {
            Some(socket) => socket,
            None => rinze_socket::default_socket()?,
        };
        Ok(Arc::new(Service {
            launcher: Launcher::new(options.program, socket.clone()),
            socket,
            stalled: AtomicBool::new(false),
        }))
    }

    fn connect(&self) -> Result<Connection, Error> {
        match Connection::open(&self.socket, REQUEST_TIMEOUT) {
            Ok(c) => {
                self.launcher.connected();
                Ok(c)
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(Error::Unavailable(e.to_string()))
            }
            Err(_) => Err(match self.launcher.not_connected() {
                Blocked::Starting => Error::Starting,
                Blocked::Unavailable(reason) => Error::Unavailable(reason),
            }),
        }
    }

    /// 起動を待っている間に、待ち受けを始めたか、起動に失敗したかを確かめる。待たずに戻る。
    pub fn poll(&self) -> Status {
        match self.connect() {
            Ok(_) => Status::Ready,
            Err(Error::Starting) => Status::Starting,
            Err(e) => Status::Unavailable(e.to_string()),
        }
    }
}

pub struct Client {
    service: Arc<Service>,
    connection: Option<Connection>,
    trail: PreeditTrail,
    last_left_context: Option<LeftContext>,
    // 待ち切れなかった応答で rinzed が確定したもの。次に返す応答の `commit` の前に付ける
    late_commit: String,
}

impl Client {
    pub fn new(service: Arc<Service>) -> Self {
        Client {
            service,
            connection: None,
            trail: PreeditTrail::default(),
            last_left_context: None,
            late_commit: String::new(),
        }
    }

    /// `surrounding` は入力先が知らせたカーソルの前後の文字列。取れなければ `None` で、左文脈は空にする。
    pub fn key(&mut self, key: &str, surrounding: Option<Surrounding>) -> Result<Response, Error> {
        let left = surrounding.map(|s| self.trail.left_context(s));
        let left_text = left.as_ref().map_or(String::new(), |l| l.text.clone());
        self.last_left_context = left;
        self.call("key", vec![key.into(), left_text.into()])
    }

    /// 入力途中のものを確定せずに捨てる。入力モードは変えない。
    pub fn discard(&mut self) -> Result<Response, Error> {
        self.call("discard", vec![])
    }

    /// 入力途中のものを表示どおりに確定する。入力モードは変えない。
    pub fn flush(&mut self) -> Result<Response, Error> {
        self.call("flush", vec![])
    }

    /// 入力途中のものを確定してから、モードを切り替えるキーを送って `target` にする。
    /// 返す応答の `commit` は、途中で確定したものをすべて合わせたもの。
    pub fn switch_mode(&mut self, target: Mode) -> Result<Response, Error> {
        let mut response = self.flush()?;
        let mut commit = std::mem::take(&mut response.commit);
        for _ in 0..MAX_SWITCH_KEYS {
            let Some(key) = switch_key(response.mode, target) else {
                break;
            };
            response = match self.call("key", vec![key.into(), "".into()]) {
                Ok(response) => response,
                Err(e) => {
                    self.late_commit.insert_str(0, &commit);
                    return Err(e);
                }
            };
            commit.push_str(&response.commit);
        }
        response.commit = commit;
        Ok(response)
    }

    pub fn last_left_context(&self) -> Option<&LeftContext> {
        self.last_left_context.as_ref()
    }

    // Delayed のときは表示を変えない。ほかの失敗では rinzed 側の入力状態が無いか失われていて、フロントエンドも未確定文字列を消す
    fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Response, Error> {
        let result = self.exchange(method, params);
        match &result {
            Ok(response) => self.trail.displayed(&response.preedit),
            Err(Error::Delayed(_)) => {}
            Err(_) => self.trail.displayed(""),
        }
        result
    }

    fn exchange(&mut self, method: &str, params: Vec<Value>) -> Result<Response, Error> {
        let connection = match &mut self.connection {
            Some(c) => c,
            None => self.connection.insert(self.service.connect()?),
        };
        if connection.has_unanswered() {
            let replies = match connection.settle(STALLED_TIMEOUT) {
                Ok(replies) => replies,
                Err(e) => {
                    self.connection = None;
                    return Err(Error::Lost(match e {
                        CallError::Broken(e) => format!("rinzed との接続が切れた: {e}"),
                        _ => "rinzed が応答しないので、入力途中のものを捨てた".into(),
                    }));
                }
            };
            self.service.stalled.store(false, Ordering::Relaxed);
            for reply in replies.iter().flatten() {
                if let Ok(late) = Response::parse(reply) {
                    self.late_commit.push_str(&late.commit);
                }
            }
        }
        let stalled = self.service.stalled.load(Ordering::Relaxed);
        let timeout = if stalled {
            STALLED_TIMEOUT
        } else {
            REQUEST_TIMEOUT
        };
        let value = match connection.call(method, params, timeout) {
            Ok(value) => value,
            Err(CallError::NoResponse) if !stalled => {
                self.service.stalled.store(true, Ordering::Relaxed);
                return Err(Error::Delayed(format!(
                    "rinzed が {} 秒以内に応答しなかった。入力途中のものは残してあり、次の打鍵で応答を待ち直す",
                    timeout.as_secs()
                )));
            }
            Err(CallError::NoResponse) => {
                self.connection = None;
                return Err(Error::Lost(format!(
                    "rinzed が {} 秒以内に応答しなかった",
                    timeout.as_secs()
                )));
            }
            Err(CallError::Broken(e)) => {
                self.connection = None;
                return Err(Error::Lost(format!("rinzed との接続が切れた: {e}")));
            }
            Err(CallError::Rejected(e)) => {
                return Err(Error::Lost(format!("rinzed が要求を受け付けなかった: {e}")));
            }
        };
        self.service.stalled.store(false, Ordering::Relaxed);
        let mut response = Response::parse(&value).map_err(|e| {
            self.connection = None;
            Error::Lost(format!("rinzed の応答を読めない: {e}"))
        })?;
        response
            .commit
            .insert_str(0, &std::mem::take(&mut self.late_commit));
        Ok(response)
    }
}

fn switch_key(current: Mode, target: Mode) -> Option<&'static str> {
    use Mode::*;
    Some(match (current, target) {
        _ if current == target => return None,
        (Ascii | FullwidthLatin, _) => "ctrl-j",
        (_, Ascii) => "l",
        (_, FullwidthLatin) => "L",
        (_, HalfwidthKatakana) => "ctrl-q",
        (_, Hiragana | Katakana) => "q",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn どのモードからどのモードへも切り替えのキーで着く() {
        use Mode::*;
        let all = [Hiragana, Katakana, HalfwidthKatakana, Ascii, FullwidthLatin];
        // docs/usage/modes.md の「入力モード」の表と同じ切り替え
        let after = |mode: Mode, key: &str| match (mode, key) {
            (Ascii | FullwidthLatin, "ctrl-j") => Hiragana,
            (Hiragana, "q") => Katakana,
            (Katakana | HalfwidthKatakana, "q") => Hiragana,
            (Hiragana | Katakana, "ctrl-q") => HalfwidthKatakana,
            (Hiragana | Katakana | HalfwidthKatakana, "l") => Ascii,
            (Hiragana | Katakana | HalfwidthKatakana, "L") => FullwidthLatin,
            other => panic!("{other:?} は表に無い"),
        };
        for from in all {
            for to in all {
                let mut mode = from;
                let mut keys = Vec::new();
                while let Some(key) = switch_key(mode, to) {
                    keys.push(key);
                    assert!(
                        keys.len() <= MAX_SWITCH_KEYS,
                        "{from:?} から {to:?}: {keys:?}"
                    );
                    mode = after(mode, key);
                }
                assert_eq!(mode, to, "{from:?} から {to:?}: {keys:?}");
            }
        }
    }
}
