use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use rmpv::Value;

const MSGPACK_RPC_REQUEST: u64 = 0;
const MSGPACK_RPC_RESPONSE: u64 = 1;

// 待ち切れなかった応答は捨てずに、次の要求の前に読む。rinzed は 1 接続の要求を順に処理するので、応答も送った順に来る
pub(crate) struct Connection {
    stream: UnixStream,
    received: Vec<u8>,
    next_msgid: u32,
    unanswered: VecDeque<u32>,
}

pub(crate) enum CallError {
    /// 時間内に応答が無かった。要求は rinzed に届いていて、応答は [`Connection::settle`] で読める。
    NoResponse,
    Broken(String),
    Rejected(String),
}

fn io_error(e: io::Error) -> CallError {
    match e.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => CallError::NoResponse,
        _ => CallError::Broken(e.to_string()),
    }
}

fn truncated(e: &rmpv::decode::Error) -> bool {
    match e {
        rmpv::decode::Error::InvalidMarkerRead(e) | rmpv::decode::Error::InvalidDataRead(e) => {
            e.kind() == io::ErrorKind::UnexpectedEof
        }
        _ => false,
    }
}

impl Connection {
    pub(crate) fn open(socket: &Path, timeout: Duration) -> io::Result<Self> {
        let stream = UnixStream::connect(socket)?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Connection {
            stream,
            received: Vec::new(),
            next_msgid: 0,
            unanswered: VecDeque::new(),
        })
    }

    pub(crate) fn has_unanswered(&self) -> bool {
        !self.unanswered.is_empty()
    }

    /// 待ち切れなかった要求の応答を、`timeout` まで待って送った順に返す。rinzed が拒んだ要求は `Err` の理由になる。
    pub(crate) fn settle(
        &mut self,
        timeout: Duration,
    ) -> Result<Vec<Result<Value, String>>, CallError> {
        let deadline = Instant::now() + timeout;
        let mut replies = Vec::new();
        while self.has_unanswered() {
            replies.push(self.receive(deadline)?);
        }
        Ok(replies)
    }

    /// 待ち切れなかった応答を [`Connection::settle`] で読み終えてから呼ぶ。
    pub(crate) fn call(
        &mut self,
        method: &str,
        params: Vec<Value>,
        timeout: Duration,
    ) -> Result<Value, CallError> {
        debug_assert!(!self.has_unanswered());
        let msgid = self.next_msgid;
        self.next_msgid = self.next_msgid.wrapping_add(1);
        let message = Value::Array(vec![
            MSGPACK_RPC_REQUEST.into(),
            msgid.into(),
            method.into(),
            Value::Array(params),
        ]);
        let mut bytes = Vec::new();
        rmpv::encode::write_value(&mut bytes, &message)
            .map_err(|e| CallError::Broken(e.to_string()))?;
        // 書きかけで止まった要求のあとには続けて書けないので、書けなければ時間切れでも接続を捨てさせる
        self.stream
            .write_all(&bytes)
            .map_err(|e| CallError::Broken(e.to_string()))?;
        self.unanswered.push_back(msgid);
        self.receive(Instant::now() + timeout)?
            .map_err(CallError::Rejected)
    }

    fn receive(&mut self, deadline: Instant) -> Result<Result<Value, String>, CallError> {
        loop {
            while let Some(reply) = self.take_message()? {
                let Some([kind, id, error, result]) = reply.as_array().map(Vec::as_slice) else {
                    return Err(CallError::Broken("応答が msgpack-rpc の形でない".into()));
                };
                let expected = self.unanswered.front().copied().map(u64::from);
                if kind.as_u64() != Some(MSGPACK_RPC_RESPONSE) || id.as_u64() != expected {
                    continue;
                }
                self.unanswered.pop_front();
                return Ok(if error.is_nil() {
                    Ok(result.clone())
                } else {
                    Err(error.to_string())
                });
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(CallError::NoResponse);
            }
            self.stream
                .set_read_timeout(Some(remaining))
                .map_err(io_error)?;
            let mut chunk = [0; 4096];
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(CallError::Broken("rinzed が接続を閉じた".into())),
                Ok(n) => self.received.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(io_error(e)),
            }
        }
    }

    // 時間切れで途中までしか読めなかった応答は、残りが届くまで `received` に置いておく
    fn take_message(&mut self) -> Result<Option<Value>, CallError> {
        let mut rest = self.received.as_slice();
        match rmpv::decode::read_value(&mut rest) {
            Ok(value) => {
                let used = self.received.len() - rest.len();
                self.received.drain(..used);
                Ok(Some(value))
            }
            Err(e) if truncated(&e) => Ok(None),
            Err(e) => Err(CallError::Broken(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(msgid: u32, preedit: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        let message = Value::Array(vec![
            MSGPACK_RPC_RESPONSE.into(),
            msgid.into(),
            Value::Nil,
            preedit.into(),
        ]);
        rmpv::encode::write_value(&mut bytes, &message).unwrap();
        bytes
    }

    fn request_id(server: &mut UnixStream) -> u32 {
        let request = rmpv::decode::read_value(server).unwrap();
        request.as_array().unwrap()[1].as_u64().unwrap() as u32
    }

    fn pair() -> (Connection, UnixStream) {
        let (client, server) = UnixStream::pair().unwrap();
        let connection = Connection {
            stream: client,
            received: Vec::new(),
            next_msgid: 0,
            unanswered: VecDeque::new(),
        };
        (connection, server)
    }

    #[test]
    fn 待ち切れなかった応答は次の要求の前に順に読み_途中で切れた応答も続きを待つ() {
        let (mut connection, mut server) = pair();
        let short = Duration::from_millis(50);
        assert!(matches!(
            connection.call("key", vec![], short),
            Err(CallError::NoResponse)
        ));
        let first = request_id(&mut server);
        let late = reply(first, "▼漢字");
        let (head, tail) = late.split_at(late.len() / 2);
        server.write_all(head).unwrap();
        assert!(matches!(
            connection.settle(short),
            Err(CallError::NoResponse)
        ));
        server.write_all(tail).unwrap();
        let replies = connection.settle(short).ok().unwrap();
        assert_eq!(replies, [Ok(Value::from("▼漢字"))]);
        assert!(!connection.has_unanswered());

        let handle = std::thread::spawn(move || {
            let second = request_id(&mut server);
            server.write_all(&reply(second, "▼感じ")).unwrap();
            server
        });
        let answer = connection.call("key", vec![], Duration::from_secs(5));
        assert_eq!(answer.ok(), Some(Value::from("▼感じ")));
        handle.join().unwrap();
    }
}
