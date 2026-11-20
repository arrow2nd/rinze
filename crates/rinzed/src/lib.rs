//! 常駐プロセス。辞書と zenz を 1 つずつ持ち、接続ごとに [`Session`] を作って msgpack-rpc で応答する。
//!
//! メソッド:
//! - `key(key: string, left_context: string) -> response`
//!   `key` は 1 文字か `space` `enter` `backspace` `ctrl-g` `ctrl-j` `ctrl-q`。
//! - `flush() -> response` 入力途中のものを表示どおりに確定する。モードは変えない。
//! - `reset() -> response` `flush` したうえで、モードをひらがなに戻す。
//! - `choose(version: int, word: string, left_context: string) -> response` 見出しの入力中に、その読みの候補 `word` で確定する。
//!   `version` が最後の応答の版と違うか、`word` が候補に無ければ何もせず `handled` を false で返す。
//! - `lookup(reading: string, left_context: string) -> [string]` 送りがなの無い読みの候補を Space で変換したときと同じ順で返す。
//!   接続の入力状態に触れない。
//!
//! `response` は `{handled, commit, preedit, candidates, minibuffer, mode, passthrough, reading, version}` の map。
//! `candidates` は一覧の表示中だけ `{items: [[label, word], ...], page, pages}`、
//! `minibuffer` は辞書登録の入力欄か辞書から消すかの問いがあるときだけ `{prompt, text, preedit}` で、それ以外は nil。
//! `mode` は `hiragana` `katakana` `halfwidth-katakana` `ascii` `fullwidth-latin` のどれか。
//! `reading` は見出しの入力中 (▽) だけ Space で変換する読みで、それ以外は nil。`version` は要求を処理するたびに増える。

mod file;
pub mod history;
pub mod learning;
pub mod user_dict;

use std::io::{BufReader, BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use history::History;
use rinze_dict::Dictionary;
use rinze_engine::{Converter, Key, Midashi, Mode, Pick, Query, Response, Session};
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
    pub learning: Learning,
    scorer: Mutex<Scorer<'m>>,
}

struct Scorer<'m> {
    zenz: Zenz<'m>,
    last: Option<Scored>,
}

struct Scored {
    left_context: String,
    reading: String,
    surfaces: Vec<String>,
    scores: Vec<f64>,
}

pub enum Learning {
    Off,
    On(Mutex<History>),
}

/// ユーザー辞書の候補にシステム辞書の候補を続け、重複を 1 つにまとめる。ユーザー辞書で外した語はシステム辞書から取らない。
pub fn lookup(dict: &Dictionary, user: &UserDictionary, midashi: &Midashi) -> Vec<String> {
    let key = midashi.key();
    let ignored = user.ignored(&key);
    let mut words: Vec<String> = user.lookup(&key).map(String::from).collect();
    for word in dict.lookup(&key) {
        if !words.iter().any(|w| w == word) && !ignored.iter().any(|i| i == word) {
            words.push(word.to_string());
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
    pub fn new(dict: Dictionary, user: UserDictionary, zenz: Zenz<'m>, learning: Learning) -> Self {
        Self {
            dict,
            user: Mutex::new(user),
            learning,
            scorer: Mutex::new(Scorer { zenz, last: None }),
        }
    }

    fn scorer(&self) -> std::sync::MutexGuard<'_, Scorer<'m>> {
        self.scorer.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn user(&self) -> std::sync::MutexGuard<'_, UserDictionary> {
        self.user.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn history(&self) -> Option<std::sync::MutexGuard<'_, History>> {
        match &self.learning {
            Learning::Off => None,
            Learning::On(history) => Some(history.lock().unwrap_or_else(|e| e.into_inner())),
        }
    }

    /// 候補が 2 件以上あれば zenz の点数に学習の加点を足して並べ替える。abbrev の候補は辞書の順に、学習の加点だけで並べ替える。
    pub fn candidates(&self, query: &Query) -> Vec<String> {
        let words = lookup(&self.dict, &self.user(), query.midashi);
        if words.len() < 2 {
            return words;
        }
        let key = query.midashi.key();
        let choices = self
            .history()
            .map_or_else(Vec::new, |h| h.choices(&key).to_vec());
        let bonus = |word: &str| learning::bonus(&choices, word, query.left_context);
        if let Midashi::Abbrev(_) = query.midashi {
            let unscored = vec![0.0; words.len()];
            return learning::rank(words, &unscored, bonus);
        }
        let started = Instant::now();
        match self.score(query.left_context, query.midashi, &words) {
            Ok(scores) => {
                log!(
                    "rinzed: {key} {} 件 {:.1}ms",
                    words.len(),
                    started.elapsed().as_secs_f64() * 1e3
                );
                learning::rank(words, &scores, bonus)
            }
            Err(e) => {
                log!("rinzed: {key} の点数づけに失敗したので辞書の順で返す: {e}");
                words
            }
        }
    }

    /// 送りがなを含めた表記の、左文脈に対する対数尤度を `words` と同じ順で返す。
    /// 点数は左文脈と読みと表記だけで決まるので、直前と同じ問いには直前の点数を返す。
    pub fn score(
        &self,
        left_context: &str,
        midashi: &Midashi,
        words: &[String],
    ) -> rinze_zenz::Result<Vec<f64>> {
        let Midashi::Kana { reading, okuri } = midashi else {
            return Err("abbrev の見出しは読みが無いので点数をつけない".into());
        };
        let okuri = okuri.as_ref().map_or("", |o| o.kana.as_str());
        let surfaces: Vec<String> = words.iter().map(|w| format!("{w}{okuri}")).collect();
        let reading = format!("{reading}{okuri}");
        let mut scorer = self.scorer();
        if let Some(scored) = scorer.last.as_ref().filter(|s| {
            s.left_context == left_context && s.reading == reading && s.surfaces == surfaces
        }) {
            return Ok(scored.scores.clone());
        }
        let scores = scorer.zenz.score(left_context, &reading, &surfaces)?;
        scorer.last = Some(Scored {
            left_context: left_context.to_string(),
            reading,
            surfaces,
            scores: scores.clone(),
        });
        Ok(scores)
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
        let generated =
            match self
                .scorer()
                .zenz
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
        let user = self.user();
        let ignored = user.ignored(&query.midashi.key());
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
                || ignored.iter().any(|i| i == stem)
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
        match self.user().purge(midashi, word, &self.dict) {
            Ok(true) => log!("rinzed: {key} /{word}/ を辞書から消した"),
            Ok(false) => log!("rinzed: {key} /{word}/ は消してある"),
            Err(e) => log!("rinzed: {key} /{word}/ を消せなかった: {e}"),
        }
        if let Some(mut history) = self.history() {
            match history.forget(&key, word) {
                Ok(true) => log!("rinzed: {key} /{word}/ を学習の履歴から消した"),
                Ok(false) => {}
                Err(e) => log!("rinzed: {key} /{word}/ を学習の履歴から消せなかった: {e}"),
            }
        }
    }

    /// 最初に出した候補をそのまま確定したときと、辞書の候補でない語 (生成した表記) は、並べ替えに使わないので記録しない。
    pub fn committed(&self, midashi: &Midashi, word: &str, pick: Pick) {
        if pick == Pick::First || matches!(self.learning, Learning::Off) {
            return;
        }
        let in_dictionary = lookup(&self.dict, &self.user(), midashi)
            .iter()
            .any(|w| w == word);
        let Some(mut history) = self.history().filter(|_| in_dictionary) else {
            return;
        };
        let key = midashi.key();
        let started = Instant::now();
        match history.record(&key, word, now_millis()) {
            Ok(()) => log!(
                "rinzed: {key} /{word}/ を学習の履歴に書いた {:.1}ms",
                started.elapsed().as_secs_f64() * 1e3
            ),
            Err(e) => log!("rinzed: {key} /{word}/ を学習の履歴に書けなかった: {e}"),
        }
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
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

    fn committed(&mut self, midashi: &Midashi, word: &str, pick: Pick) {
        self.0.committed(midashi, word, pick)
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
            Ok(r) => (Value::Nil, r),
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
) -> Result<Value, String> {
    let params = params.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let converter = &mut SharedConverter(backend);
    let response = match method {
        "key" => {
            let [key, left] = params else {
                return Err("key は (key, left_context) を受け取る".into());
            };
            let key = parse_key(key.as_str().unwrap_or(""))?;
            session.handle(key, left.as_str().unwrap_or(""), converter)
        }
        "flush" => session.flush(converter),
        "reset" => session.reset(converter),
        "choose" => {
            let [version, word, left] = params else {
                return Err("choose は (version, word, left_context) を受け取る".into());
            };
            let version = version.as_u64().ok_or("version が数でない")?;
            let word = word.as_str().unwrap_or("");
            session.choose(version, word, left.as_str().unwrap_or(""), converter)
        }
        "lookup" => {
            let [reading, left] = params else {
                return Err("lookup は (reading, left_context) を受け取る".into());
            };
            let midashi = Midashi::Kana {
                reading: reading.as_str().unwrap_or("").to_string(),
                okuri: None,
            };
            let query = Query {
                left_context: left.as_str().unwrap_or(""),
                midashi: &midashi,
            };
            let words = backend.candidates(&query);
            return Ok(Value::Array(words.into_iter().map(Value::from).collect()));
        }
        _ => return Err(format!("未知のメソッド: {method}")),
    };
    Ok(encode_response(&response))
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
        (
            "reading".into(),
            r.reading.as_deref().map_or(Value::Nil, Value::from),
        ),
        ("version".into(), r.version.into()),
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
        user.purge(&at, "遭", &dict).unwrap();
        assert_eq!(
            lookup(&dict, &user, &at),
            ["会", "合"],
            "ユーザー辞書から消した語は出ない"
        );
    }

    #[test]
    fn システム辞書の語を消すと次の変換にも開き直したあとにも出ず_登録し直せば出る() {
        let dir = std::env::temp_dir().join(format!("rinzed-purge-shared-{}", std::process::id()));
        let path = dir.join("user.jisyo");
        let dict = Dictionary::parse("あした /明日/朝/晨;あさ/\n");
        let asita = kana("あした", None);
        let mut user = UserDictionary::open(path.clone()).unwrap();
        user.purge(&asita, "明日", &dict).unwrap();
        user.purge(&asita, "晨", &dict).unwrap();
        assert_eq!(lookup(&dict, &user, &asita), ["朝"]);
        let reopened = UserDictionary::open(path.clone()).unwrap();
        assert_eq!(lookup(&dict, &reopened, &asita), ["朝"]);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("あした /(skk-ignore-dic-word \"明日\" \"晨\\073あさ\")/\n"),
            "ddskk と同じ書式で外した語を書く"
        );
        user.register(&asita, "明日").unwrap();
        assert_eq!(lookup(&dict, &user, &asita), ["明日", "朝"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn 辞書に無い語を消すとその読みの候補から外す() {
        let dict = Dictionary::parse("");
        let mut user = UserDictionary::in_memory();
        let mayo = kana("まよねーず", None);
        user.purge(&mayo, "マヨネーズ", &dict).unwrap();
        assert_eq!(user.ignored("まよねーず"), ["マヨネーズ"]);
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
