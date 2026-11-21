//! SKK 辞書。見出し (送りありなら `あt` のように読みと送りがなの子音) から候補を引く。

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

/// 見出しと候補の文字列を 1 つの `text` に詰め、見出しの順に並べた `entries` を二分探索で引く。
/// 文字列ごとに確保しないので、7 つの辞書を合わせても HashMap と String で持つときの 3 割ほどで済む。
#[derive(Debug, Default)]
pub struct Dictionary {
    text: String,
    entries: Vec<Entry>,
    written: Vec<Span>,
    ignored: HashMap<String, Vec<String>>,
}

/// `text` の中の範囲。
#[derive(Debug, Clone, Copy)]
struct Span {
    start: u32,
    end: u32,
}

impl Span {
    fn push(text: &mut String, s: &str) -> Self {
        let start = index(text.len());
        text.push_str(s);
        Self {
            start,
            end: index(text.len()),
        }
    }

    fn of(self, text: &str) -> &str {
        &text[self.start as usize..self.end as usize]
    }
}

#[derive(Debug)]
struct Entry {
    key: Span,
    written: Range<u32>,
}

/// 読み込んだ 1 行。`source` は渡された辞書の番号。
struct ParsedLine {
    key: Span,
    written: Range<u32>,
    source: usize,
}

/// UTF-8 として読めなければ EUC-JP として読む。SKK-JISYO.L などの配布物は EUC-JP。
pub fn decode(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => encoding_rs::EUC_JP.decode(bytes).0,
    }
}

impl Dictionary {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::parse(&decode(bytes))
    }

    pub fn parse(text: &str) -> Self {
        Self::merged([text])
    }

    /// 辞書を渡した順に読んで 1 つにする。同じ見出しの候補は、先の辞書の候補のあとに重複なく続ける。
    /// 重複した語の書き方 (注釈) は先の辞書のものを残す。1 つの辞書に同じ見出しの行が 2 つあれば、後の行を使う。
    pub fn merged<T: AsRef<str>>(texts: impl IntoIterator<Item = T>) -> Self {
        let mut text = String::new();
        let mut ignored: HashMap<String, Vec<String>> = HashMap::new();
        let mut lines = Vec::new();
        let mut parsed = Vec::new();
        for (source, dict) in texts.into_iter().enumerate() {
            let mut ignored_here = HashMap::new();
            for (key, rest) in dict
                .as_ref()
                .lines()
                .filter(|line| !line.starts_with(';'))
                .filter_map(|line| line.split_once(" /"))
            {
                let line = parse_line(rest);
                if !line.written.is_empty() {
                    let start = index(parsed.len());
                    parsed.extend(line.written.iter().map(|w| Span::push(&mut text, w)));
                    lines.push(ParsedLine {
                        key: Span::push(&mut text, key),
                        written: start..index(parsed.len()),
                        source,
                    });
                }
                if !line.ignored.is_empty() {
                    ignored_here.insert(key, line.ignored);
                }
            }
            for (key, words) in ignored_here {
                let merged = ignored.entry(key.to_string()).or_default();
                for word in words {
                    if !merged.contains(&word) {
                        merged.push(word);
                    }
                }
            }
        }
        lines.sort_by(|a, b| a.key.of(&text).cmp(b.key.of(&text)));
        let mut entries = Vec::new();
        let mut written: Vec<Span> = Vec::with_capacity(parsed.len());
        for run in lines.chunk_by(|a, b| a.key.of(&text) == b.key.of(&text)) {
            let start = written.len();
            for (i, line) in run.iter().enumerate() {
                if run
                    .get(i + 1)
                    .is_some_and(|next| next.source == line.source)
                {
                    continue;
                }
                for &span in &parsed[line.written.start as usize..line.written.end as usize] {
                    let word = word_of(span.of(&text));
                    if !written[start..]
                        .iter()
                        .any(|w| word_of(w.of(&text)) == word)
                    {
                        written.push(span);
                    }
                }
            }
            entries.push(Entry {
                key: run[0].key,
                written: index(start)..index(written.len()),
            });
        }
        text.shrink_to_fit();
        entries.shrink_to_fit();
        written.shrink_to_fit();
        Self {
            text,
            entries,
            written,
            ignored,
        }
    }

    /// 見出しの候補の、辞書での書き方 (注釈を含む) を辞書の順に返す。
    fn written_of(&self, key: &str) -> impl Iterator<Item = &str> {
        let found = self
            .entries
            .binary_search_by(|e| e.key.of(&self.text).cmp(key))
            .ok();
        let range = found.map_or(0..0, |i| {
            let written = &self.entries[i].written;
            written.start as usize..written.end as usize
        });
        self.written[range].iter().map(|span| span.of(&self.text))
    }

    /// 見出しの候補を辞書の順に、注釈を除いて返す。
    pub fn lookup(&self, key: &str) -> impl Iterator<Item = &str> {
        self.written_of(key).map(word_of)
    }

    /// 候補 `word` の辞書での書き方 (注釈を含む)。
    pub fn written(&self, key: &str, word: &str) -> Option<&str> {
        self.written_of(key)
            .find(|written| word_of(written) == word)
    }

    /// 見出しの行の `(skk-ignore-dic-word ...)` に書かれた語 (注釈を除く)。
    pub fn ignored(&self, key: &str) -> &[String] {
        self.ignored.get(key).map_or(&[], Vec::as_slice)
    }
}

const OKURI_ARI: &str = ";; okuri-ari entries.";
const OKURI_NASI: &str = ";; okuri-nasi entries.";
// ddskk は候補に `(skk-ignore-dic-word "語" ...)` があると、引数の語を同じ見出しのほかの辞書の候補から除く
const IGNORE_DIC_WORD: &str = "(skk-ignore-dic-word";

/// SKK 辞書の節。abbrev の見出し (`file`) も送りなしの節に入る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    OkuriAri,
    OkuriNasi,
}

/// 語を辞書から消す方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purge<'a> {
    /// 行から語を除く。
    Delete,
    /// 行から語を除き、ほかの辞書にもあるその語を ddskk と同じく `(skk-ignore-dic-word "語")` で候補から外す。
    /// 中身はほかの辞書での語の書き方 (注釈を含む)。ddskk は注釈を含めた書き方で候補と照らし合わせる。
    Ignore(&'a str),
}

/// SKK 辞書のテキストに語を足したテキストを返す。ユーザー辞書の書き換えに使う。
///
/// 同じ見出しの行があれば語をその候補の先頭に置き (既にあれば先頭へ移し)、無ければ `section` の節の先頭に行を足す。
/// `(skk-ignore-dic-word ...)` に語があれば外す。ほかの行は注釈や式も含めてそのまま残す。`key` は送りありなら `あt` のように子音で終わる。
/// 見出しか語が SKK 辞書に書けない (見出しが空、`;` で始まる、空白を含む。語が `/` `;` を含む、式や送りがなブロックと見分けられない) なら None を返す。
pub fn add_entry(text: &str, section: Section, key: &str, word: &str) -> Option<String> {
    let unwritable_key = key.is_empty() || key.starts_with(';') || key.contains([' ', '\n']);
    if unwritable_key
        || word.is_empty()
        || word.contains(['/', ';', '\n'])
        || word.starts_with(['(', '['])
    {
        return None;
    }
    let mut lines = lines_of(text);
    let prefix = format!("{key} /");
    match lines.iter().position(|l| l.starts_with(&prefix)) {
        Some(index) => {
            let mut items = vec![word.to_string()];
            let mut in_okuri_block = false;
            for item in items_of(&lines[index][prefix.len()..]) {
                in_okuri_block = (in_okuri_block || item.starts_with('[')) && item != "]";
                if let Some(args) = ignore_args(item) {
                    let args: Vec<String> =
                        args.into_iter().filter(|a| word_of(a) != word).collect();
                    if !args.is_empty() {
                        items.push(ignore_item(&args));
                    }
                } else if in_okuri_block || item == "]" || word_of(item) != word {
                    items.push(item.to_string());
                }
            }
            lines[index] = line_of(key, &items);
        }
        None => insert_line(&mut lines, section, line_of(key, &[word.to_string()])),
    }
    Some(join_lines(lines))
}

/// SKK 辞書のテキストから、見出し `key` の語 `word` を送りがなブロックの中も含めて除いたテキストを返す。
/// 候補が無くなった行と送りがなブロックは消す。
/// [`Purge::Ignore`] では、行の `(skk-ignore-dic-word ...)` を 1 つにまとめて語を足し、ddskk と同じく候補の先頭に置く。行が無ければ `section` の節に作る。
/// テキストが変わらない (消す語が無い、既に外している) なら None を返す。
pub fn purge_entry(
    text: &str,
    section: Section,
    key: &str,
    word: &str,
    purge: Purge,
) -> Option<String> {
    let prefix = format!("{key} /");
    let mut lines = lines_of(text);
    let index = lines.iter().position(|l| l.starts_with(&prefix));
    let items: Vec<&str> =
        index.map_or_else(Vec::new, |i| items_of(&lines[i][prefix.len()..]).collect());
    let is_word = |item: &str| !item.starts_with(['[', '(']) && item != "]";
    let mut kept: Vec<String> = Vec::new();
    let mut ignored: Vec<String> = Vec::new();
    let mut removed = false;
    for &item in &items {
        if is_word(item) && word_of(item) == word {
            removed = true;
            continue;
        }
        if item == "]" && kept.last().is_some_and(|last| last.starts_with('[')) {
            kept.pop();
            continue;
        }
        match (purge, ignore_args(item)) {
            (Purge::Ignore(_), Some(args)) => ignored.extend(args),
            _ => kept.push(item.to_string()),
        }
    }
    if let Purge::Ignore(written) = purge {
        let already_ignored = ignored.iter().any(|a| word_of(a) == word);
        if already_ignored && !removed {
            return None;
        }
        if !already_ignored {
            ignored.push(written.to_string());
        }
        kept.insert(0, ignore_item(&ignored));
    } else if !removed {
        return None;
    }
    match (index, kept.is_empty()) {
        (Some(i), true) => {
            lines.remove(i);
        }
        (Some(i), false) => lines[i] = line_of(key, &kept),
        (None, _) => insert_line(&mut lines, section, line_of(key, &kept)),
    }
    Some(join_lines(lines))
}

fn lines_of(text: &str) -> Vec<String> {
    if text.is_empty() {
        [";; -*- coding: utf-8 -*-", OKURI_ARI, OKURI_NASI]
            .map(String::from)
            .to_vec()
    } else {
        text.lines().map(String::from).collect()
    }
}

fn insert_line(lines: &mut Vec<String>, section: Section, line: String) {
    let header = match section {
        Section::OkuriAri => OKURI_ARI,
        Section::OkuriNasi => OKURI_NASI,
    };
    let at = lines
        .iter()
        .position(|l| l == header)
        .map_or(lines.len(), |i| i + 1);
    lines.insert(at, line);
}

fn items_of(rest: &str) -> impl Iterator<Item = &str> {
    rest.split('/').filter(|i| !i.is_empty())
}

fn line_of(key: &str, items: &[String]) -> String {
    format!("{key} /{}/", items.join("/"))
}

fn join_lines(lines: Vec<String>) -> String {
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn index(n: usize) -> u32 {
    u32::try_from(n).expect("辞書は 4 GiB に収まる")
}

fn word_of(written: &str) -> &str {
    written.split(';').next().unwrap_or("")
}

struct Line<'a> {
    written: Vec<&'a str>,
    ignored: Vec<String>,
}

fn parse_line(rest: &str) -> Line<'_> {
    let mut line = Line {
        written: Vec::new(),
        ignored: Vec::new(),
    };
    let mut in_okuri_block = false;
    for item in rest.split('/') {
        // 送りありエントリの `[る/図/]` は送りがなごとの候補の重複なので読み飛ばす
        if item.starts_with('[') {
            in_okuri_block = true;
            continue;
        }
        if item == "]" {
            in_okuri_block = false;
            continue;
        }
        if in_okuri_block {
            continue;
        }
        if let Some(args) = ignore_args(item) {
            line.ignored
                .extend(args.iter().map(|a| word_of(a).to_string()));
            continue;
        }
        let word = word_of(item);
        // `(concat ...)` などの Lisp 式の候補は評価しないので捨てる
        if word.is_empty() || word.starts_with('(') {
            continue;
        }
        line.written.push(item);
    }
    line
}

// 引数は Emacs Lisp の文字列で、ddskk は `;` を `\073` のようにエスケープして書く
fn ignore_args(item: &str) -> Option<Vec<String>> {
    let body = item.strip_prefix(IGNORE_DIC_WORD)?.strip_suffix(')')?;
    let mut args = Vec::new();
    let mut chars = body.chars().peekable();
    loop {
        match chars.next()? {
            ' ' => continue,
            '"' => {}
            _ => return None,
        }
        let mut arg = String::new();
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => match chars.next()? {
                    'n' => arg.push('\n'),
                    'r' => arg.push('\r'),
                    d @ '0'..='7' => {
                        let mut code = d.to_digit(8)?;
                        for _ in 0..2 {
                            code = code * 8 + chars.next()?.to_digit(8)?;
                        }
                        arg.push(char::from_u32(code)?);
                    }
                    c => arg.push(c),
                },
                c => arg.push(c),
            }
        }
        args.push(arg);
        if chars.peek().is_none() {
            return Some(args);
        }
    }
}

// ddskk の `skk-compose-ignore-word-sub-quote-char` と同じく、辞書に書けない文字を含む引数だけをエスケープする
fn ignore_item(args: &[String]) -> String {
    let quoted: Vec<String> = args
        .iter()
        .map(|arg| {
            if !arg.contains(['/', '\n', '\r', '"', ';']) {
                return format!("\"{arg}\"");
            }
            let mut out = String::from("\"");
            for c in arg.chars() {
                match c {
                    ';' => out.push_str("\\073"),
                    '/' => out.push_str("\\057"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        })
        .collect();
    format!("{IGNORE_DIC_WORD} {})", quoted.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words<'a>(dict: &'a Dictionary, key: &str) -> Vec<&'a str> {
        dict.lookup(key).collect()
    }

    #[test]
    fn 注釈と送りがなブロックと式を除き重複をまとめる() {
        let dict = Dictionary::parse(
            ";; okuri-ari entries.\n\
             あt /合;fit/会/[っ/合/会/]/遭/(concat \"x\")/合/\n\
             あした /明日/朝/\n",
        );
        assert_eq!(words(&dict, "あt"), ["合", "会", "遭"]);
        assert_eq!(words(&dict, "あした"), ["明日", "朝"]);
        assert!(words(&dict, "ない").is_empty());
    }

    #[test]
    fn 辞書を合わせると同じ見出しの候補を渡した順に重複なく並べる() {
        let dict = Dictionary::merged([
            "たなか /田中;姓/田仲;姓/\nあした /明日/\n",
            "たなか /田中;人名/多中/\nしんじゅく /新宿/\nあした /(skk-ignore-dic-word \"朝\")/\n",
            "たなか /田中町/多中/\n",
        ]);
        assert_eq!(words(&dict, "たなか"), ["田中", "田仲", "多中", "田中町"]);
        assert_eq!(words(&dict, "しんじゅく"), ["新宿"]);
        assert_eq!(
            dict.written("たなか", "田中"),
            Some("田中;姓"),
            "重複した語は先の辞書の書き方を残す"
        );
        assert_eq!(dict.ignored("あした"), ["朝"]);
    }

    #[test]
    fn 一つの辞書に同じ見出しの行が二つあれば後の行を使う() {
        let dict = Dictionary::merged(["あさ /麻/\nあした /明日/\nあさ /朝/浅/\n", "あさ /麻/\n"]);
        assert_eq!(words(&dict, "あさ"), ["朝", "浅", "麻"]);
        assert_eq!(words(&dict, "あした"), ["明日"]);
    }

    #[test]
    fn 注釈つきの候補は辞書での書き方を引ける() {
        let dict = Dictionary::parse("あした /明日/晨;あさ/\n");
        assert_eq!(dict.written("あした", "晨"), Some("晨;あさ"));
        assert_eq!(dict.written("あした", "明日"), Some("明日"));
        assert_eq!(dict.written("あした", "朝"), None);
    }

    #[test]
    fn skk_ignore_dic_word_に書いた語を見出しごとに引ける() {
        let dict = Dictionary::parse(
            "あした /朝/(skk-ignore-dic-word \"明日\" \"晨\\073あさ\")/\nあさ /朝/\n",
        );
        assert_eq!(words(&dict, "あした"), ["朝"]);
        assert_eq!(dict.ignored("あした"), ["明日", "晨"]);
        assert!(dict.ignored("あさ").is_empty());
    }

    #[test]
    fn 語を足すと節の先頭か同じ見出しの候補の先頭に入る() {
        let text = add_entry("", Section::OkuriNasi, "まよねーず", "マヨネーズ").unwrap();
        let text = add_entry(&text, Section::OkuriAri, "あe", "和").unwrap();
        let text = add_entry(&text, Section::OkuriAri, "あe", "会").unwrap();
        let text = add_entry(&text, Section::OkuriAri, "あe", "和").unwrap();
        assert_eq!(
            text,
            ";; -*- coding: utf-8 -*-\n\
             ;; okuri-ari entries.\n\
             あe /和/会/\n\
             ;; okuri-nasi entries.\n\
             まよねーず /マヨネーズ/\n"
        );
        let dict = Dictionary::parse(&text);
        assert_eq!(words(&dict, "あe"), ["和", "会"]);
        assert_eq!(words(&dict, "まよねーず"), ["マヨネーズ"]);
    }

    #[test]
    fn 語を足しても他の行と注釈と式は残す() {
        let text = ";; 手で書いた行\nあさ /朝;morning/(concat \"a\")/\nよる /夜/\n";
        assert_eq!(
            add_entry(text, Section::OkuriNasi, "あさ", "麻").unwrap(),
            ";; 手で書いた行\nあさ /麻/朝;morning/(concat \"a\")/\nよる /夜/\n"
        );
        assert_eq!(
            add_entry(text, Section::OkuriNasi, "ひる", "昼").unwrap(),
            ";; 手で書いた行\nあさ /朝;morning/(concat \"a\")/\nよる /夜/\nひる /昼/\n",
            "節の見出しが無ければ末尾に足す"
        );
        assert_eq!(
            add_entry("はかr /計/図/[る/図/]/\n", Section::OkuriAri, "はかr", "図").unwrap(),
            "はかr /図/計/[る/図/]/\n",
            "送りがなブロックの中の同じ語は消さない"
        );
    }

    #[test]
    fn 辞書の書式で書けない見出しと語は足さない() {
        for word in ["a/b", "注;釈", "(concat)", "[る", ""] {
            assert_eq!(
                add_entry("", Section::OkuriNasi, "あ", word),
                None,
                "{word}"
            );
        }
        for key in ["", ";x", "a b"] {
            assert_eq!(add_entry("", Section::OkuriNasi, key, "語"), None, "{key}");
        }
    }

    #[test]
    fn 英字の見出しは送りなしの節に入る() {
        let text = add_entry("", Section::OkuriNasi, "file", "ファイル").unwrap();
        assert_eq!(
            text,
            ";; -*- coding: utf-8 -*-\n\
             ;; okuri-ari entries.\n\
             ;; okuri-nasi entries.\n\
             file /ファイル/\n"
        );
    }

    #[test]
    fn 語を除くと候補が無くなった行と送りがなブロックも消える() {
        let text = ";; okuri-ari entries.\nあt /会;meet/合/[っ/会/]/[た/会/合/]/\n;; okuri-nasi entries.\nまよ /真夜/\n";
        let delete = |key, word| purge_entry(text, Section::OkuriNasi, key, word, Purge::Delete);
        assert_eq!(
            delete("あt", "会").unwrap(),
            ";; okuri-ari entries.\nあt /合/[た/合/]/\n;; okuri-nasi entries.\nまよ /真夜/\n"
        );
        assert_eq!(
            delete("まよ", "真夜").unwrap(),
            ";; okuri-ari entries.\nあt /会;meet/合/[っ/会/]/[た/会/合/]/\n;; okuri-nasi entries.\n"
        );
        assert_eq!(delete("まよ", "魔余"), None);
        assert_eq!(delete("よる", "夜"), None);
    }

    // 期待する行は、ddskk 17.2 (skk-server-host を nil にした状態) で共有辞書の語を X で消した、同じ順の操作のあとの個人辞書の行
    #[test]
    fn ほかの辞書にもある語を消すと_skk_ignore_dic_word_を候補の先頭に書く() {
        let ignore = |text, word, written| {
            purge_entry(
                text,
                Section::OkuriNasi,
                "あした",
                word,
                Purge::Ignore(written),
            )
        };
        assert_eq!(
            ignore("", "明日", "明日").unwrap(),
            ";; -*- coding: utf-8 -*-\n\
             ;; okuri-ari entries.\n\
             ;; okuri-nasi entries.\n\
             あした /(skk-ignore-dic-word \"明日\")/\n"
        );
        assert_eq!(
            ignore(
                "あした /晨;あさ/朝/(skk-ignore-dic-word \"明日\")/\n",
                "朝",
                "朝"
            )
            .unwrap(),
            "あした /(skk-ignore-dic-word \"明日\" \"朝\")/晨;あさ/\n"
        );
        assert_eq!(
            ignore(
                "あした /晨;あさ/明日/(skk-ignore-dic-word \"明日\")/\n",
                "晨",
                "晨;あさ"
            )
            .unwrap(),
            "あした /(skk-ignore-dic-word \"明日\" \"晨\\073あさ\")/明日/\n"
        );
        assert_eq!(
            ignore("あした /(skk-ignore-dic-word \"明日\")/\n", "明日", "明日"),
            None,
            "既に外している語では書き直さない"
        );
    }

    #[test]
    fn 外した語を足すと_skk_ignore_dic_word_からも除く() {
        assert_eq!(
            add_entry(
                "あした /晨;あさ/(skk-ignore-dic-word \"明日\" \"朝\")/\n",
                Section::OkuriNasi,
                "あした",
                "明日"
            )
            .unwrap(),
            "あした /明日/晨;あさ/(skk-ignore-dic-word \"朝\")/\n"
        );
        assert_eq!(
            add_entry(
                "あした /(skk-ignore-dic-word \"晨\\073あさ\")/\n",
                Section::OkuriNasi,
                "あした",
                "晨"
            )
            .unwrap(),
            "あした /晨/\n"
        );
    }

    #[test]
    fn euc_jp_の辞書を読める() {
        let (bytes, _, _) = encoding_rs::EUC_JP.encode("あした /明日/\n");
        assert_eq!(words(&Dictionary::from_bytes(&bytes), "あした"), ["明日"]);
    }
}
