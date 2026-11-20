//! 学習の履歴。見出しごとに、選び直して確定した候補と、最後に選び直した時刻を持つ。
//!
//! UTF-8 のテキストで、1 行に `見出し<TAB>候補<TAB>時刻` (時刻は UNIX 時刻のミリ秒) を書く。`;;` で始まる行は読まない。
//! 書き込むのは rinzed だけで、書くたびにロックを取って最新の内容に記録を足すので、別の rinzed の記録を消さない。

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use crate::file;
use crate::learning::{self, Choice};

const HEADER: &str =
    ";; rinze の学習の履歴。見出し<TAB>候補<TAB>最後に選び直した時刻 (UNIX 時刻のミリ秒)\n";

#[derive(Debug)]
pub struct History {
    storage: Storage,
    choices: Choices,
}

type Choices = BTreeMap<String, Vec<Choice>>;

#[derive(Debug)]
enum Storage {
    File(PathBuf),
    Memory,
}

impl History {
    /// ファイルが無ければ空の履歴として始め、最初の記録で作る。
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let text = file::read_or_empty(&path)?;
        Ok(Self {
            choices: parse(&text),
            storage: Storage::File(path),
        })
    }

    pub fn in_memory() -> Self {
        Self {
            storage: Storage::Memory,
            choices: Choices::new(),
        }
    }

    pub fn choices(&self, key: &str) -> &[Choice] {
        self.choices.get(key).map_or(&[], Vec::as_slice)
    }

    /// 見出しか候補がタブか改行を含んで書けなければ、何もせずにエラーを返す。
    pub fn record(&mut self, key: &str, word: &str, at: u64) -> io::Result<()> {
        if [key, word].iter().any(|s| s.contains(['\t', '\n'])) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{key} /{word}/ はタブか改行を含むので履歴に書けない"),
            ));
        }
        self.rewrite(|choices| {
            learning::record(choices.entry(key.to_string()).or_default(), word, at);
        })
    }

    /// 履歴に無ければ何もせずに false を返す。
    pub fn forget(&mut self, key: &str, word: &str) -> io::Result<bool> {
        let mut found = false;
        self.rewrite(|choices| {
            if let Some(words) = choices.get_mut(key) {
                let before = words.len();
                words.retain(|c| c.word != word);
                found = words.len() < before;
                if words.is_empty() {
                    choices.remove(key);
                }
            }
        })?;
        Ok(found)
    }

    fn rewrite(&mut self, edit: impl FnOnce(&mut Choices)) -> io::Result<()> {
        match &self.storage {
            Storage::Memory => edit(&mut self.choices),
            Storage::File(path) => {
                let text = file::rewrite(path, |latest| {
                    let mut choices = parse(latest);
                    edit(&mut choices);
                    Ok(format(&choices))
                })?;
                self.choices = parse(&text);
            }
        }
        Ok(())
    }
}

fn parse(text: &str) -> Choices {
    let mut choices = Choices::new();
    for line in text.lines().filter(|l| !l.starts_with(";;")) {
        let mut fields = line.split('\t');
        let (Some(key), Some(word), Some(Ok(last)), None) = (
            fields.next(),
            fields.next(),
            fields.next().map(str::parse),
            fields.next(),
        ) else {
            continue;
        };
        learning::record(choices.entry(key.to_string()).or_default(), word, last);
    }
    choices
}

fn format(choices: &Choices) -> String {
    let mut text = HEADER.to_string();
    for (key, words) in choices {
        for Choice { word, last } in words {
            text.push_str(&format!("{key}\t{word}\t{last}\n"));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("rinze-history-{name}-{}", std::process::id()))
            .join("nested")
            .join("history.tsv")
    }

    fn words(history: &History, key: &str) -> Vec<(String, u64)> {
        history
            .choices(key)
            .iter()
            .map(|c| (c.word.clone(), c.last))
            .collect()
    }

    #[test]
    fn 記録はファイルに残り_開き直しても読める() {
        let path = temp_path("reopen");
        let mut history = History::open(path.clone()).unwrap();
        history.record("ひろし", "浩", 10).unwrap();
        history.record("あe", "和", 20).unwrap();
        history.record("ひろし", "浩", 30).unwrap();
        let reopened = History::open(path.clone()).unwrap();
        assert_eq!(words(&reopened, "ひろし"), [("浩".to_string(), 30)]);
        assert_eq!(words(&reopened, "あe"), [("和".to_string(), 20)]);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{HEADER}あe\t和\t20\nひろし\t浩\t30\n")
        );
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            !names.iter().any(|n| n.contains(".tmp.")),
            "一時ファイルを残さない: {names:?}"
        );
        fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn 消した記録はファイルからも消え_無い記録を消しても何も変えない() {
        let path = temp_path("forget");
        let mut history = History::open(path.clone()).unwrap();
        history.record("ひろし", "浩", 10).unwrap();
        history.record("ひろし", "宏", 20).unwrap();
        assert!(history.forget("ひろし", "宏").unwrap());
        assert!(!history.forget("ひろし", "宏").unwrap());
        assert_eq!(words(&history, "ひろし"), [("浩".to_string(), 10)]);
        assert!(history.forget("ひろし", "浩").unwrap());
        assert!(history.choices("ひろし").is_empty());
        let reopened = History::open(path.clone()).unwrap();
        assert!(reopened.choices("ひろし").is_empty());
        fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn 別の書き手の記録を消さない() {
        let path = temp_path("merge");
        let mut a = History::open(path.clone()).unwrap();
        let mut b = History::open(path.clone()).unwrap();
        a.record("ひろし", "浩", 10).unwrap();
        b.record("あe", "和", 20).unwrap();
        b.record("ひろし", "宏", 30).unwrap();
        a.record("ひろし", "浩", 40).unwrap();
        let reopened = History::open(path.clone()).unwrap();
        assert_eq!(
            words(&reopened, "ひろし"),
            [("浩".to_string(), 40), ("宏".to_string(), 30)]
        );
        assert_eq!(words(&reopened, "あe"), [("和".to_string(), 20)]);
        assert_eq!(
            words(&a, "あe"),
            [("和".to_string(), 20)],
            "書いたときに相手の記録も読む"
        );
        fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn 読めない行は飛ばし_タブを含む語は書かない() {
        let mut history = History::in_memory();
        assert!(history.record("あ", "a\tb", 1).is_err());
        assert!(history.choices("あ").is_empty());
        let parsed = parse("ひろし\t浩\t10\n壊れた行\nひろし\t宏\tx\nあ\tい\t1\t余り\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["ひろし"].len(), 1);
    }
}
