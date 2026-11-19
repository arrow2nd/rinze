//! SKK 辞書。見出し (送りありなら `あt` のように読みと送りがなの子音) から候補を引く。

use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Dictionary {
    entries: HashMap<String, Vec<String>>,
}

impl Dictionary {
    /// UTF-8 として読めなければ EUC-JP として読む。SKK-JISYO.L などの配布物は EUC-JP。
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::parse(text),
            Err(_) => Self::parse(&encoding_rs::EUC_JP.decode(bytes).0),
        }
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
    fn euc_jp_の辞書を読める() {
        let (bytes, _, _) = encoding_rs::EUC_JP.encode("あした /明日/\n");
        assert_eq!(Dictionary::from_bytes(&bytes).lookup("あした"), ["明日"]);
    }
}
