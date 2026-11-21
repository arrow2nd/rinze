//! msgpack の 1 メッセージの境界を、大きさと入れ子の深さに上限を置いて順に調べる。
//! 文字列や配列のヘッダに巨大な長さが書かれていても、本体を読む前に拒む。

use std::fmt;
use std::io::{self, BufRead};

/// 1 メッセージの最大バイト数 (1 MiB)。通常の応答は数 KiB に収まる。
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
/// 配列とマップの入れ子の最大段数。
pub const MAX_NESTING_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStatus {
    NeedMore,
    Complete(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    TooLarge,
    TooDeep,
    Invalid,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::TooLarge => write!(f, "メッセージが大きすぎる"),
            FrameError::TooDeep => write!(f, "メッセージの入れ子が深すぎる"),
            FrameError::Invalid => write!(f, "msgpack の形式が正しくない"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<FrameError> for io::Error {
    fn from(e: FrameError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e)
    }
}

enum Token {
    /// ヘッダのあとに `payload` バイトの本体の続く値。
    Payload { header: usize, payload: usize },
    /// ヘッダのあとに `items` 個の要素の続く配列かマップ。
    Container { header: usize, items: usize },
}

/// 途中まで届いたバッファを先頭から読み直さずに、msgpack の 1 メッセージの終端を探す。
#[derive(Debug)]
pub struct FrameScanner {
    max_bytes: usize,
    pos: usize,
    skip: usize,
    stack: Vec<usize>,
    total_remaining: usize,
}

impl FrameScanner {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            pos: 0,
            skip: 0,
            stack: vec![1],
            total_remaining: 1,
        }
    }

    pub fn reset(&mut self) {
        self.pos = 0;
        self.skip = 0;
        self.stack.clear();
        self.stack.push(1);
        self.total_remaining = 1;
    }

    /// `buf` に追加された分だけ進み、1 メッセージが揃えばそのバイト数を返す。
    /// 完了したときは次のメッセージに備えて状態を戻す。
    pub fn scan(&mut self, buf: &[u8]) -> Result<ScanStatus, FrameError> {
        loop {
            if self.skip > 0 {
                let take = self.skip.min(buf.len() - self.pos);
                self.pos += take;
                self.skip -= take;
                if self.skip > 0 {
                    return Ok(ScanStatus::NeedMore);
                }
                self.pop_finished();
                if self.stack.is_empty() {
                    let len = self.pos;
                    self.reset();
                    return Ok(ScanStatus::Complete(len));
                }
            }
            if self.pos == buf.len() {
                return Ok(ScanStatus::NeedMore);
            }
            let Some(token) = parse_token(&buf[self.pos..])? else {
                return Ok(ScanStatus::NeedMore);
            };
            self.apply(token)?;
            if self.skip == 0 {
                self.pop_finished();
                if self.stack.is_empty() {
                    let len = self.pos;
                    self.reset();
                    return Ok(ScanStatus::Complete(len));
                }
            }
        }
    }

    fn apply(&mut self, token: Token) -> Result<(), FrameError> {
        if let Some(top) = self.stack.last_mut() {
            *top -= 1;
            self.total_remaining -= 1;
        }
        match token {
            Token::Payload { header, payload } => {
                self.pos += header;
                self.skip = payload;
                self.check_budget(0)?;
            }
            Token::Container { header, items } => {
                self.pos += header;
                if items > 0 {
                    if self.stack.len() >= MAX_NESTING_DEPTH {
                        return Err(FrameError::TooDeep);
                    }
                    self.check_budget(items)?;
                    self.stack.push(items);
                    self.total_remaining += items;
                } else {
                    self.check_budget(0)?;
                }
            }
        }
        Ok(())
    }

    fn check_budget(&self, extra_items: usize) -> Result<(), FrameError> {
        let min_total = self
            .pos
            .checked_add(self.skip)
            .and_then(|n| n.checked_add(self.total_remaining))
            .and_then(|n| n.checked_add(extra_items));
        if min_total.is_none_or(|n| n > self.max_bytes) {
            return Err(FrameError::TooLarge);
        }
        Ok(())
    }

    fn pop_finished(&mut self) {
        while self.stack.last() == Some(&0) {
            self.stack.pop();
        }
    }
}

fn parse_token(rest: &[u8]) -> Result<Option<Token>, FrameError> {
    let b = rest[0];
    let payload = |header, payload| Ok(Some(Token::Payload { header, payload }));
    let container = |header, items| Ok(Some(Token::Container { header, items }));
    match b {
        0x00..=0x7f | 0xc0 | 0xc2 | 0xc3 | 0xe0..=0xff => payload(1, 0),
        0x80..=0x8f => container(1, usize::from(b & 0x0f) * 2),
        0x90..=0x9f => container(1, usize::from(b & 0x0f)),
        0xa0..=0xbf => payload(1, usize::from(b & 0x1f)),
        0xc1 => Err(FrameError::Invalid),
        0xcc | 0xd0 => payload(1, 1),
        0xcd | 0xd1 => payload(1, 2),
        0xca | 0xce | 0xd2 => payload(1, 4),
        0xcb | 0xcf | 0xd3 => payload(1, 8),
        0xd4 => payload(1, 2),
        0xd5 => payload(1, 3),
        0xd6 => payload(1, 5),
        0xd7 => payload(1, 9),
        0xd8 => payload(1, 17),
        0xc4 | 0xd9 => parse_len_payload(rest, 1, 0),
        0xc5 | 0xda => parse_len_payload(rest, 2, 0),
        0xc6 | 0xdb => parse_len_payload(rest, 4, 0),
        0xc7 => parse_len_payload(rest, 1, 1),
        0xc8 => parse_len_payload(rest, 2, 1),
        0xc9 => parse_len_payload(rest, 4, 1),
        0xdc => parse_len_container(rest, 2, 1),
        0xdd => parse_len_container(rest, 4, 1),
        0xde => parse_len_container(rest, 2, 2),
        0xdf => parse_len_container(rest, 4, 2),
    }
}

fn read_be_len(rest: &[u8], width: usize) -> Option<usize> {
    let bytes = rest.get(1..1 + width)?;
    let n = match width {
        1 => u32::from(bytes[0]),
        2 => u32::from(u16::from_be_bytes([bytes[0], bytes[1]])),
        4 => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        _ => unreachable!(),
    };
    usize::try_from(n).ok()
}

fn parse_len_payload(rest: &[u8], width: usize, extra: usize) -> Result<Option<Token>, FrameError> {
    let Some(len) = read_be_len(rest, width) else {
        return Ok(None);
    };
    let payload = len.checked_add(extra).ok_or(FrameError::TooLarge)?;
    Ok(Some(Token::Payload {
        header: 1 + width,
        payload,
    }))
}

fn parse_len_container(
    rest: &[u8],
    width: usize,
    scale: usize,
) -> Result<Option<Token>, FrameError> {
    let Some(len) = read_be_len(rest, width) else {
        return Ok(None);
    };
    let items = len.checked_mul(scale).ok_or(FrameError::TooLarge)?;
    Ok(Some(Token::Container {
        header: 1 + width,
        items,
    }))
}

/// `reader` から 1 メッセージ分のバイト列を `out` へ読む。
/// メッセージの先頭で EOF に着けば `Ok(false)`、途中で切れたり上限を超えたりすれば `Err` を返す。
pub fn read_frame(
    reader: &mut impl BufRead,
    max_bytes: usize,
    out: &mut Vec<u8>,
) -> io::Result<bool> {
    out.clear();
    let mut scanner = FrameScanner::new(max_bytes);
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if out.is_empty() {
                return Ok(false);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "メッセージの途中で接続が閉じた",
            ));
        }
        let before = out.len();
        let remaining_cap = max_bytes.saturating_sub(before);
        let take = available.len().min(remaining_cap);
        if take == 0 {
            return Err(FrameError::TooLarge.into());
        }
        out.extend_from_slice(&available[..take]);
        match scanner.scan(out).map_err(io::Error::from)? {
            ScanStatus::Complete(len) => {
                reader.consume(len - before);
                out.truncate(len);
                return Ok(true);
            }
            ScanStatus::NeedMore => {
                reader.consume(take);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn 分割されたフレームを先頭から読み直さずに切り出せる() {
        // [0, 1, "key", ["a", ""]]
        let msg = b"\x94\x00\x01\xa3key\x92\xa1a\xa0";
        let mut scanner = FrameScanner::new(MAX_MESSAGE_BYTES);
        let mut buf = Vec::new();
        for &byte in &msg[..msg.len() - 1] {
            buf.push(byte);
            assert_eq!(scanner.scan(&buf), Ok(ScanStatus::NeedMore));
        }
        buf.push(*msg.last().unwrap());
        assert_eq!(scanner.scan(&buf), Ok(ScanStatus::Complete(msg.len())));
    }

    #[test]
    fn 連続した複数フレームを余分に消費せずに読める() {
        let mut stream = Vec::new();
        stream.extend_from_slice(b"\x92\x01\xa2hi");
        stream.extend_from_slice(b"\x81\xa1k\xc3");
        let mut reader = Cursor::new(stream);
        let mut frame = Vec::new();
        assert!(read_frame(&mut reader, MAX_MESSAGE_BYTES, &mut frame).unwrap());
        assert_eq!(frame, b"\x92\x01\xa2hi");
        assert!(read_frame(&mut reader, MAX_MESSAGE_BYTES, &mut frame).unwrap());
        assert_eq!(frame, b"\x81\xa1k\xc3");
        assert!(!read_frame(&mut reader, MAX_MESSAGE_BYTES, &mut frame).unwrap());
    }

    #[test]
    fn 上限を超える長さヘッダは本体を読む前に拒む() {
        // str32 で 2 GiB を宣言する 5 バイトだけの入力
        let huge_str = b"\xdb\x7f\xff\xff\xff";
        let mut scanner = FrameScanner::new(MAX_MESSAGE_BYTES);
        assert_eq!(scanner.scan(huge_str), Err(FrameError::TooLarge));

        // array32 で 2M 要素を宣言
        let huge_arr = b"\xdd\x00\x20\x00\x00";
        let mut scanner = FrameScanner::new(MAX_MESSAGE_BYTES);
        assert_eq!(scanner.scan(huge_arr), Err(FrameError::TooLarge));
    }

    #[test]
    fn 入れ子が深すぎるメッセージと不正なマーカーを拒む() {
        let deep = vec![0x91u8; MAX_NESTING_DEPTH + 1];
        let mut scanner = FrameScanner::new(MAX_MESSAGE_BYTES);
        assert_eq!(scanner.scan(&deep), Err(FrameError::TooDeep));

        let mut scanner = FrameScanner::new(MAX_MESSAGE_BYTES);
        assert_eq!(scanner.scan(&[0xc1]), Err(FrameError::Invalid));
    }
}
