//! SKK 辞書。見出し (送りありなら `あt` のように読みと送りがなの子音) から候補を引く。

use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Dictionary {
    entries: HashMap<String, Vec<String>>,
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
        let entries = text
            .lines()
            .filter(|line| !line.starts_with(';'))
            .filter_map(|line| line.split_once(" /"))
            .map(|(key, rest)| (key.to_string(), parse_candidates(rest)))
            .filter(|(_, cands)| !cands.is_empty())
            .collect();
        Self { entries }
    }

    pub fn lookup(&self, key: &str) -> &[String] {
        self.entries.get(key).map_or(&[], Vec::as_slice)
    }
}

const OKURI_ARI: &str = ";; okuri-ari entries.";
const OKURI_NASI: &str = ";; okuri-nasi entries.";

/// SKK 辞書のテキストに語を足したテキストを返す。ユーザー辞書の書き換えに使う。
///
/// 同じ見出しの行があれば語をその候補の先頭に置き (既にあれば先頭へ移し)、無ければ送りあり・なしの節の先頭に行を足す。
/// ほかの行は注釈や式も含めてそのまま残す。`key` は送りありなら `あt` のように子音で終わる。
/// 語が SKK の候補として書けない (`/` `;` を含む、式や送りがなブロックと見分けられない) なら None を返す。
pub fn add_entry(text: &str, key: &str, word: &str) -> Option<String> {
    if word.is_empty() || word.contains(['/', ';', '\n']) || word.starts_with(['(', '[']) {
        return None;
    }
    let mut lines: Vec<String> = if text.is_empty() {
        [";; -*- coding: utf-8 -*-", OKURI_ARI, OKURI_NASI]
            .map(String::from)
            .to_vec()
    } else {
        text.lines().map(String::from).collect()
    };
    let prefix = format!("{key} /");
    if let Some(line) = lines.iter_mut().find(|l| l.starts_with(&prefix)) {
        let mut rest = String::new();
        let mut in_okuri_block = false;
        for item in line[prefix.len()..].split('/').filter(|i| !i.is_empty()) {
            in_okuri_block = (in_okuri_block || item.starts_with('[')) && item != "]";
            if in_okuri_block || item == "]" || item.split(';').next() != Some(word) {
                rest.push_str(item);
                rest.push('/');
            }
        }
        *line = format!("{prefix}{word}/{rest}");
    } else {
        let okuri_ari = key.ends_with(|c: char| c.is_ascii_lowercase());
        let header = if okuri_ari { OKURI_ARI } else { OKURI_NASI };
        let at = lines
            .iter()
            .position(|l| l == header)
            .map_or(lines.len(), |i| i + 1);
        lines.insert(at, format!("{prefix}{word}/"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    Some(out)
}

fn parse_candidates(rest: &str) -> Vec<String> {
    let mut cands: Vec<String> = Vec::new();
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
        let word = item.split(';').next().unwrap_or("");
        // `(concat ...)` などの Lisp 式の候補は評価しないので捨てる
        if in_okuri_block || word.is_empty() || word.starts_with('(') {
            continue;
        }
        if !cands.iter().any(|c| c == word) {
            cands.push(word.to_string());
        }
    }
    cands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 注釈と送りがなブロックと式を除き重複をまとめる() {
        let dict = Dictionary::parse(
            ";; okuri-ari entries.\n\
             あt /合;fit/会/[っ/合/会/]/遭/(concat \"x\")/合/\n\
             あした /明日/朝/\n",
        );
        assert_eq!(dict.lookup("あt"), ["合", "会", "遭"]);
        assert_eq!(dict.lookup("あした"), ["明日", "朝"]);
        assert!(dict.lookup("ない").is_empty());
    }

    #[test]
    fn 語を足すと節の先頭か同じ見出しの候補の先頭に入る() {
        let text = add_entry("", "まよねーず", "マヨネーズ").unwrap();
        let text = add_entry(&text, "あe", "和").unwrap();
        let text = add_entry(&text, "あe", "会").unwrap();
        let text = add_entry(&text, "あe", "和").unwrap();
        assert_eq!(
            text,
            ";; -*- coding: utf-8 -*-\n\
             ;; okuri-ari entries.\n\
             あe /和/会/\n\
             ;; okuri-nasi entries.\n\
             まよねーず /マヨネーズ/\n"
        );
        let dict = Dictionary::parse(&text);
        assert_eq!(dict.lookup("あe"), ["和", "会"]);
        assert_eq!(dict.lookup("まよねーず"), ["マヨネーズ"]);
    }

    #[test]
    fn 語を足しても他の行と注釈と式は残す() {
        let text = ";; 手で書いた行\nあさ /朝;morning/(concat \"a\")/\nよる /夜/\n";
        assert_eq!(
            add_entry(text, "あさ", "麻").unwrap(),
            ";; 手で書いた行\nあさ /麻/朝;morning/(concat \"a\")/\nよる /夜/\n"
        );
        assert_eq!(
            add_entry(text, "ひる", "昼").unwrap(),
            ";; 手で書いた行\nあさ /朝;morning/(concat \"a\")/\nよる /夜/\nひる /昼/\n",
            "節の見出しが無ければ末尾に足す"
        );
        assert_eq!(
            add_entry("はかr /計/図/[る/図/]/\n", "はかr", "図").unwrap(),
            "はかr /図/計/[る/図/]/\n",
            "送りがなブロックの中の同じ語は消さない"
        );
    }

    #[test]
    fn 辞書の書式で書けない語は足さない() {
        for word in ["a/b", "注;釈", "(concat)", "[る", ""] {
            assert_eq!(add_entry("", "あ", word), None, "{word}");
        }
    }

    #[test]
    fn euc_jp_の辞書を読める() {
        let (bytes, _, _) = encoding_rs::EUC_JP.encode("あした /明日/\n");
        assert_eq!(Dictionary::from_bytes(&bytes).lookup("あした"), ["明日"]);
    }
}
