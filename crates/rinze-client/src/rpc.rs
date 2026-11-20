use std::io::{self, BufReader, BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use rmpv::Value;

const MSGPACK_RPC_REQUEST: u64 = 0;
const MSGPACK_RPC_RESPONSE: u64 = 1;

pub(crate) struct Connection {
    reader: BufReader<UnixStream>,
    writer: BufWriter<UnixStream>,
    next_msgid: u32,
}

pub(crate) enum CallError {
    // 遅れて来る応答と次の要求の応答を取り違えないよう、この接続は捨てる
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

fn decode_error(e: rmpv::decode::Error) -> CallError {
    match e {
        rmpv::decode::Error::InvalidMarkerRead(e) | rmpv::decode::Error::InvalidDataRead(e) => {
            io_error(e)
        }
        e => CallError::Broken(e.to_string()),
    }
}

impl Connection {
    pub(crate) fn open(socket: &Path, timeout: Duration) -> io::Result<Self> {
        let stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Connection {
            reader: BufReader::new(stream.try_clone()?),
            writer: BufWriter::new(stream),
            next_msgid: 0,
        })
    }

    pub(crate) fn stream(&self) -> &UnixStream {
        self.writer.get_ref()
    }

    pub(crate) fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, CallError> {
        let msgid = self.next_msgid;
        self.next_msgid = self.next_msgid.wrapping_add(1);
        let message = Value::Array(vec![
            MSGPACK_RPC_REQUEST.into(),
            msgid.into(),
            method.into(),
            Value::Array(params),
        ]);
        rmpv::encode::write_value(&mut self.writer, &message).map_err(|e| io_error(e.into()))?;
        self.writer.flush().map_err(io_error)?;
        loop {
            let reply = rmpv::decode::read_value(&mut self.reader).map_err(decode_error)?;
            let Some([kind, id, error, result]) = reply.as_array().map(Vec::as_slice) else {
                return Err(CallError::Broken("応答が msgpack-rpc の形でない".into()));
            };
            if kind.as_u64() != Some(MSGPACK_RPC_RESPONSE) || id.as_u64() != Some(msgid.into()) {
                continue;
            }
            if !error.is_nil() {
                return Err(CallError::Rejected(error.to_string()));
            }
            return Ok(result.clone());
        }
    }
}
