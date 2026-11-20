//! 常駐プロセス。辞書と zenz を 1 つずつ持ち、接続ごとに [`Session`] を作って msgpack-rpc で応答する。
//!
//! メソッド:
//! - `key(key: string, left_context: string) -> response`
//!   `key` は 1 文字か `space` `enter` `backspace` `ctrl-g` `ctrl-j` `ctrl-q`。
//! - `flush() -> response` 入力途中のものを表示どおりに確定する。モードは変えない。
//! - `reset() -> response` `flush` したうえで、モードをひらがなに戻す。
//!
//! `response` は `{handled, commit, preedit, candidates, minibuffer, mode, passthrough}` の map。
//! `candidates` は一覧の表示中だけ `{items: [[label, word], ...], page, pages}`、
//! `minibuffer` は辞書登録の入力欄か辞書から消すかの問いがあるときだけ `{prompt, text, preedit}` で、それ以外は nil。
//! `mode` は `hiragana` `katakana` `halfwidth-katakana` `ascii` `fullwidth-latin` のどれか。

pub mod user_dict;

use std::io::{BufReader, BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::time::Instant;

use rinze_dict::Dictionary;
use rinze_engine::{Converter, Key, Midashi, Mode, Query, Response, Session};
use rinze_zenz::Zenz;
use rmpv::Value;
use user_dict::UserDictionary;

/// 標準エラー出力へ 1 行書く。書けなくても無視する。
///
/// 自動起動した rinzed の標準エラー出力は起動したフロントエンドへのパイプなので、そのフロントエンドが終わると書けなくなる。
/// `eprintln!` はそこで panic し、ほかのフロントエンドの接続を処理しているスレッドごと落ちる。
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        let _ = std::io::Write::write_fmt(
            &mut std::io::stderr(),
            format_args!("{}\n", format_args!($($arg)*)),
        );
    }};
}

const GENERATE_WIDTH: usize = 8;
const MAX_LOG_LIKELIHOOD_BELOW_BEST: f64 = 6.0;
const KANJI_LIKE_KATAKANA: [char; 2] = ['ヵ', 'ヶ'];
const MSGPACK_RPC_REQUEST: u64 = 0;
const MSGPACK_RPC_RESPONSE: u64 = 1;

pub struct Backend<'m> {
    pub dict: Dictionary,
    pub user: Mutex<UserDictionary>,
    pub zenz: Mutex<Zenz<'m>>,
}

/// ユーザー辞書の候補にシステム辞書の候補を続け、重複を 1 つにまとめる。
pub fn lookup(dict: &Dictionary, user: &UserDictionary, midashi: &Midashi) -> Vec<String> {
    let key = midashi.key();
    let mut words = user.lookup(&key).to_vec();
    for word in dict.lookup(&key) {
        if !words.contains(word) {
            words.push(word.clone());
        }
    }
    words
}

/// 表記 `surface` が読み `reading` (ひらがな) に対応しうるか。
/// 表記の中のかなは読みの同じ位置のかなと一致し、かな以外 (漢字や英字) の連なりは読みの 1 字以上に当たるものとして調べる。
pub fn reads_as(surface: &str, reading: &str) -> bool {
    let reading: Vec<char> = reading.chars().collect();
    let mut reach = vec![false; reading.len() + 1];
    reach[0] = true;
    let mut chars = surface.chars().peekable();
    while let Some(c) = chars.next() {
        let mut next = vec![false; reading.len() + 1];
        match hiragana(c) {
            Some(kana) => {
                for i in (0..reading.len()).filter(|&i| reach[i] && reading[i] == kana) {
                    next[i + 1] = true;
                }
            }
            None => {
                while chars.peek().is_some_and(|&c| hiragana(c).is_none()) {
                    chars.next();
                }
                if let Some(first) = reach.iter().position(|&r| r) {
                    next[first + 1..].fill(true);
                }
            }
        }
        reach = next;
    }
    reach[reading.len()]
}

fn hiragana(c: char) -> Option<char> {
    match c {
        c if KANJI_LIKE_KATAKANA.contains(&c) => None,
        'ぁ'..='ゖ' | 'ー' => Some(c),
        'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60),
        _ => None,
    }
}

impl<'m> Backend<'m> {
    pub fn new(dict: Dictionary, user: UserDictionary, zenz: Zenz<'m>) -> Self {
        Self {
            dict,
            user: Mutex::new(user),
            zenz: Mutex::new(zenz),
        }
    }

    fn zenz(&self) -> std::sync::MutexGuard<'_, Zenz<'m>> {
        self.zenz.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn user(&self) -> std::sync::MutexGuard<'_, UserDictionary> {
        self.user.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 候補が 2 件以上あれば zenz の点数で並べ替える。abbrev の候補は辞書の順のまま返す。
    pub fn candidates(&self, query: &Query) -> Vec<String> {
        let words = lookup(&self.dict, &self.user(), query.midashi);
        let Midashi::Kana { reading, okuri } = query.midashi else {
            return words;
        };
        if words.len() < 2 {
            return words;
        }
        let okuri = okuri.as_ref().map_or("", |o| o.kana.as_str());
        let surfaces: Vec<String> = words.iter().map(|w| format!("{w}{okuri}")).collect();
        let reading = format!("{reading}{okuri}");
        let started = Instant::now();
        let scores = self.zenz().score(query.left_context, &reading, &surfaces);
        match scores {
            Ok(scores) => {
                log!(
                    "rinzed: {reading} {} 件 {:.1}ms",
                    words.len(),
                    started.elapsed().as_secs_f64() * 1e3
                );
                let mut order: Vec<usize> = (0..words.len()).collect();
                order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
                order.into_iter().map(|i| words[i].clone()).collect()
            }
            Err(e) => {
                log!("rinzed: {reading} の点数づけに失敗したので辞書の順で返す: {e}");
                words
            }
        }
    }

    /// zenz が生成した表記のうち、読みと送りがなに対応するものを良い順に返す。読みと同じひらがなの表記は除く。
    /// zenz は読みをかなで受け取るので、abbrev では生成しない。
    pub fn generate(&self, query: &Query) -> Vec<String> {
        let Midashi::Kana {
            reading: stem_reading,
            okuri,
        } = query.midashi
        else {
            return Vec::new();
        };
        let okuri = okuri.as_ref().map_or("", |o| o.kana.as_str());
        let reading = format!("{stem_reading}{okuri}");
        let started = Instant::now();
        let generated = match self
            .zenz()
            .generate(query.left_context, &reading, GENERATE_WIDTH)
        {
            Ok(g) => g,
            Err(e) => {
                log!("rinzed: {reading} の生成に失敗した: {e}");
                return Vec::new();
            }
        };
        let Some(best) = generated.first().map(|g| g.score) else {
            return Vec::new();
        };
        let mut words: Vec<String> = Vec::new();
        for g in generated
            .iter()
            .filter(|g| g.score >= best - MAX_LOG_LIKELIHOOD_BELOW_BEST)
        {
            let Some(stem) = g.text.strip_suffix(okuri) else {
                continue;
            };
            if stem.is_empty()
                || stem == stem_reading
                || !reads_as(stem, stem_reading)
                || words.iter().any(|w| w == stem)
            {
                continue;
            }
            words.push(stem.to_string());
        }
        log!(
            "rinzed: {reading} を生成 {:.1}ms {:?} から {:?}",
            started.elapsed().as_secs_f64() * 1e3,
            generated.iter().map(|g| &g.text).collect::<Vec<_>>(),
            words
        );
        words
    }

    pub fn register(&self, midashi: &Midashi, word: &str) {
        let key = midashi.key();
        match self.user().register(midashi, word) {
            Ok(()) => log!("rinzed: {key} /{word}/ を登録した"),
            Err(e) => log!("rinzed: {key} /{word}/ を登録できなかった: {e}"),
        }
    }

    pub fn purge(&self, midashi: &Midashi, word: &str) {
        let key = midashi.key();
        match self.user().purge(midashi, word) {
            Ok(true) => log!("rinzed: {key} /{word}/ をユーザー辞書から消した"),
            Ok(false) => log!("rinzed: {key} /{word}/ はユーザー辞書に無い"),
            Err(e) => log!("rinzed: {key} /{word}/ を消せなかった: {e}"),
        }
    }
}

struct SharedConverter<'a, 'm>(&'a Backend<'m>);

impl Converter for SharedConverter<'_, '_> {
    fn candidates(&mut self, query: &Query) -> Vec<String> {
        self.0.candidates(query)
    }

    fn generate(&mut self, query: &Query) -> Vec<String> {
        self.0.generate(query)
    }

    fn register(&mut self, midashi: &Midashi, word: &str) {
        self.0.register(midashi, word)
    }

    fn purge(&mut self, midashi: &Midashi, word: &str) {
        self.0.purge(midashi, word)
    }
}

/// 1 接続ぶんの要求を、接続が切れるまで順に処理する。
pub fn serve(stream: UnixStream, backend: &Backend<'_>) -> std::io::Result<()> {
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
        let Some([kind, msgid, method, params]) = message.as_array().map(Vec::as_slice) else {
            continue;
        };
        if kind.as_u64() != Some(MSGPACK_RPC_REQUEST) {
            continue;
        }
        let result = dispatch(&mut session, backend, method.as_str().unwrap_or(""), params);
        let (error, result) = match result {
            Ok(r) => (Value::Nil, encode_response(&r)),
            Err(e) => (Value::from(e), Value::Nil),
        };
        let reply = Value::Array(vec![
            Value::from(MSGPACK_RPC_RESPONSE),
            msgid.clone(),
            error,
            result,
        ]);
        rmpv::encode::write_value(&mut writer, &reply).map_err(std::io::Error::other)?;
        writer.flush()?;
    }
}

fn dispatch(
    session: &mut Session,
    backend: &Backend<'_>,
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
        "reset" => Ok(session.reset()),
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
        ("ctrl-j", ..) => Ok(Key::Kakutei),
        ("ctrl-q", ..) => Ok(Key::ToggleHalfwidth),
        (_, Some(c), None) => Ok(Key::Char(c)),
        _ => Err(format!("未知のキー: {name}")),
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Hiragana => "hiragana",
        Mode::Katakana => "katakana",
        Mode::HalfwidthKatakana => "halfwidth-katakana",
        Mode::Ascii => "ascii",
        Mode::FullwidthLatin => "fullwidth-latin",
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
    let minibuffer = r.minibuffer.as_ref().map_or(Value::Nil, |line| {
        Value::Map(vec![
            ("prompt".into(), line.prompt.as_str().into()),
            ("text".into(), line.text.as_str().into()),
            ("preedit".into(), line.preedit.as_str().into()),
        ])
    });
    Value::Map(vec![
        ("handled".into(), r.handled.into()),
        ("commit".into(), r.commit.as_str().into()),
        ("preedit".into(), r.preedit.as_str().into()),
        ("candidates".into(), candidates),
        ("minibuffer".into(), minibuffer),
        ("mode".into(), mode_name(r.mode).into()),
        ("passthrough".into(), r.passthrough.into()),
    ])
}

#[cfg(test)]
mod tests {
    use rinze_engine::Okuri;

    use super::*;

    fn kana(reading: &str, okuri: Option<(char, &str)>) -> Midashi {
        Midashi::Kana {
            reading: reading.into(),
            okuri: okuri.map(|(consonant, kana)| Okuri {
                consonant,
                kana: kana.into(),
            }),
        }
    }

    #[test]
    fn 送りありは読みと子音で引く() {
        let dict = Dictionary::parse("あt /合/会/\nあ /亜/\n");
        let user = UserDictionary::in_memory();
        assert_eq!(
            lookup(&dict, &user, &kana("あ", Some(('t', "っ")))),
            ["合", "会"]
        );
        assert_eq!(lookup(&dict, &user, &kana("あ", None)), ["亜"]);
        assert!(
            lookup(&dict, &user, &kana("まよねーず", None)).is_empty(),
            "辞書に無い読みに候補を作らない"
        );
    }

    #[test]
    fn abbrev_は英字の見出しで引く() {
        let dict = Dictionary::parse("file /ファイル/\n");
        let user = UserDictionary::in_memory();
        assert_eq!(
            lookup(&dict, &user, &Midashi::Abbrev("file".into())),
            ["ファイル"]
        );
    }

    #[test]
    fn ユーザー辞書の語とシステム辞書の語を重複なく合わせる() {
        let dict = Dictionary::parse("あt /合/会/\n");
        let mut user = UserDictionary::in_memory();
        let at = kana("あ", Some(('t', "っ")));
        user.register(&at, "会").unwrap();
        user.register(&at, "遭").unwrap();
        assert_eq!(lookup(&dict, &user, &at), ["遭", "会", "合"]);
        user.purge(&at, "遭").unwrap();
        assert_eq!(
            lookup(&dict, &user, &at),
            ["会", "合"],
            "ユーザー辞書から消した語は出ない"
        );
    }

    #[test]
    fn 表記が読みに対応するかを調べる() {
        for (surface, reading) in [
            ("マヨネーズ", "まよねーず"),
            ("会", "あ"),
            ("天気がいい", "てんきがいい"),
            ("ChatGPT", "ちゃっとじーぴーてぃー"),
            ("チャットGPT", "ちゃっとじーぴーてぃー"),
            ("一ヶ月", "いっかげつ"),
        ] {
            assert!(
                reads_as(surface, reading),
                "{surface} は {reading} と読める"
            );
        }
        for (surface, reading) in [
            ("マヨネーズ！", "まよねーず"),
            ("天気がよい", "てんきがいい"),
            ("あした。", "あした"),
            ("会え", "あ"),
            ("", "あ"),
        ] {
            assert!(
                !reads_as(surface, reading),
                "{surface} は {reading} と読めない"
            );
        }
    }
}
