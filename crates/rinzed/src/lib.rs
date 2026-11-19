//! 常駐プロセス。辞書と zenz を 1 つずつ持ち、接続ごとに [`Session`] を作って msgpack-rpc で応答する。
//!
//! メソッド:
//! - `key(key: string, left_context: string) -> response`
//!   `key` は 1 文字か `space` `enter` `backspace` `ctrl-g`。
//! - `flush() -> response` 入力途中のものを表示どおりに確定する。
//!
//! `response` は `{handled, commit, preedit, candidates}` の map で、`candidates` は一覧の表示中だけ
//! `{items: [[label, word], ...], page, pages}`、それ以外は nil。

use std::io::{BufReader, BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::time::Instant;

use rinze_dict::Dictionary;
use rinze_engine::{Converter, Key, Query, Response, Session};
use rinze_zenz::Zenz;
use rmpv::Value;

pub struct Backend {
    pub dict: Dictionary,
    pub zenz: Mutex<Zenz>,
}

/// 辞書の順で候補を引く。送りなしの読みが辞書に無ければ、読みのカタカナ表記を唯一の候補にする。
/// SKK-JISYO.L は「まよねーず」のような外来語を持たず、カナモードもまだ無いので、こうしないと外来語を入力できない。
pub fn lookup(dict: &Dictionary, query: &Query) -> Vec<String> {
    let Some(okuri) = query.okuri else {
        let words = dict.lookup(query.reading);
        if words.is_empty() {
            return vec![query.reading.chars().map(to_katakana).collect()];
        }
        return words.to_vec();
    };
    dict.lookup(&format!("{}{}", query.reading, okuri.consonant))
        .to_vec()
}

fn to_katakana(c: char) -> char {
    if ('ぁ'..='ゖ').contains(&c) {
        char::from_u32(c as u32 + 0x60).unwrap_or(c)
    } else {
        c
    }
}

impl Backend {
    /// 候補が 2 件以上あれば zenz の点数で並べ替える。
    pub fn candidates(&self, query: &Query) -> Vec<String> {
        let words = lookup(&self.dict, query);
        if words.len() < 2 {
            return words;
        }
        let okuri = query.okuri.map_or("", |o| o.kana.as_str());
        let surfaces: Vec<String> = words.iter().map(|w| format!("{w}{okuri}")).collect();
        let reading = format!("{}{okuri}", query.reading);
        let started = Instant::now();
        let scores = self.zenz.lock().unwrap_or_else(|e| e.into_inner()).score(
            query.left_context,
            &reading,
            &surfaces,
        );
        match scores {
            Ok(scores) => {
                eprintln!(
                    "rinzed: {reading} {} 件 {:.1}ms",
                    words.len(),
                    started.elapsed().as_secs_f64() * 1e3
                );
                let mut order: Vec<usize> = (0..words.len()).collect();
                order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
                order.into_iter().map(|i| words[i].clone()).collect()
            }
            Err(e) => {
                eprintln!("rinzed: {reading} の点数づけに失敗したので辞書の順で返す: {e}");
                words
            }
        }
    }
}

struct SharedConverter<'a>(&'a Backend);

impl Converter for SharedConverter<'_> {
    fn candidates(&mut self, query: &Query) -> Vec<String> {
        self.0.candidates(query)
    }
}

/// 1 接続ぶんの要求を、接続が切れるまで順に処理する。
pub fn serve(stream: UnixStream, backend: &Backend) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);
    let mut session = Session::default();
    loop {
        let message = match rmpv::decode::read_value(&mut reader) {
            Ok(v) => v,
            Err(rmpv::decode::Error::InvalidMarkerRead(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                return Ok(());
            }
            Err(e) => return Err(std::io::Error::other(e)),
        };
        // 要求は [0, msgid, method, params]。通知 [2, method, params] は受け付けるメソッドが無いので無視する
        let Some([kind, msgid, method, params]) = message.as_array().map(Vec::as_slice) else {
            continue;
        };
        if kind.as_u64() != Some(0) {
            continue;
        }
        let result = dispatch(&mut session, backend, method.as_str().unwrap_or(""), params);
        let (error, result) = match result {
            Ok(r) => (Value::Nil, encode_response(&r)),
            Err(e) => (Value::from(e), Value::Nil),
        };
        let reply = Value::Array(vec![Value::from(1), msgid.clone(), error, result]);
        rmpv::encode::write_value(&mut writer, &reply).map_err(std::io::Error::other)?;
        writer.flush()?;
    }
}

fn dispatch(
    session: &mut Session,
    backend: &Backend,
    method: &str,
    params: &Value,
) -> Result<Response, String> {
    let params = params.as_array().map(Vec::as_slice).unwrap_or(&[]);
    match method {
        "key" => {
            let [key, left] = params else {
                return Err("key は (key, left_context) を受け取る".into());
            };
            let key = parse_key(key.as_str().unwrap_or(""))?;
            let left = left.as_str().unwrap_or("");
            Ok(session.handle(key, left, &mut SharedConverter(backend)))
        }
        "flush" => Ok(session.flush()),
        _ => Err(format!("未知のメソッド: {method}")),
    }
}

fn parse_key(name: &str) -> Result<Key, String> {
    let mut chars = name.chars();
    match (name, chars.next(), chars.next()) {
        ("space", ..) => Ok(Key::Space),
        ("enter", ..) => Ok(Key::Enter),
        ("backspace", ..) => Ok(Key::Backspace),
        ("ctrl-g", ..) => Ok(Key::Cancel),
        (_, Some(c), None) => Ok(Key::Char(c)),
        _ => Err(format!("未知のキー: {name}")),
    }
}

fn encode_response(r: &Response) -> Value {
    let candidates = r.candidates.as_ref().map_or(Value::Nil, |page| {
        let items = page
            .items
            .iter()
            .map(|(label, word)| Value::Array(vec![label.to_string().into(), word.as_str().into()]))
            .collect();
        Value::Map(vec![
            ("items".into(), Value::Array(items)),
            ("page".into(), page.page.into()),
            ("pages".into(), page.pages.into()),
        ])
    });
    Value::Map(vec![
        ("handled".into(), r.handled.into()),
        ("commit".into(), r.commit.as_str().into()),
        ("preedit".into(), r.preedit.as_str().into()),
        ("candidates".into(), candidates),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinze_engine::Okuri;

    fn query<'a>(reading: &'a str, okuri: Option<&'a Okuri>) -> Query<'a> {
        Query {
            left_context: "",
            reading,
            okuri,
        }
    }

    #[test]
    fn 送りありは読みと子音で引く() {
        let dict = Dictionary::parse("あt /合/会/\nあ /亜/\n");
        let okuri = Okuri {
            consonant: 't',
            kana: "っ".into(),
        };
        assert_eq!(lookup(&dict, &query("あ", Some(&okuri))), ["合", "会"]);
        assert_eq!(lookup(&dict, &query("あ", None)), ["亜"]);
    }

    #[test]
    fn 辞書に無い送りなしの読みはカタカナにする() {
        let dict = Dictionary::parse("まよね /眉根/\n");
        assert_eq!(lookup(&dict, &query("まよねーず", None)), ["マヨネーズ"]);
        let okuri = Okuri {
            consonant: 'k',
            kana: "く".into(),
        };
        assert!(
            lookup(&dict, &query("まよ", Some(&okuri))).is_empty(),
            "送りありはカタカナにしない"
        );
    }
}
