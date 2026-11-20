//! ユーザー辞書。SKK 辞書と同じ書式の UTF-8 のテキストで、書き込むのは rinzed だけ。

use std::io;
use std::path::PathBuf;

use rinze_dict::{Dictionary, Purge, Section};
use rinze_engine::Midashi;

use crate::file;

#[derive(Debug)]
pub struct UserDictionary {
    storage: Storage,
    dict: Dictionary,
}

#[derive(Debug)]
enum Storage {
    File(PathBuf),
    Memory(String),
}

impl UserDictionary {
    /// ファイルが無ければ空の辞書として始め、最初の登録で作る。
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let text = file::read_or_empty(&path)?;
        Ok(Self {
            dict: Dictionary::parse(&text),
            storage: Storage::File(path),
        })
    }

    pub fn in_memory() -> Self {
        Self {
            storage: Storage::Memory(String::new()),
            dict: Dictionary::default(),
        }
    }

    pub fn lookup(&self, key: &str) -> impl Iterator<Item = &str> {
        self.dict.lookup(key)
    }

    /// 消してあり、システム辞書や生成した表記から取らない語。
    pub fn ignored(&self, key: &str) -> &[String] {
        self.dict.ignored(key)
    }

    /// 語が辞書の書式で書けなければ何もせずにエラーを返す。
    pub fn register(&mut self, midashi: &Midashi, word: &str) -> io::Result<()> {
        self.rewrite(|text| {
            rinze_dict::add_entry(text, section(midashi), &midashi.key(), word).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("「{word}」は SKK 辞書の候補として書けない"),
                )
            })
        })
    }

    /// ユーザー辞書だけにある語は行から除く。システム辞書にある語と、どちらの辞書にも無い語 (生成した表記) は、
    /// ddskk と同じく `(skk-ignore-dic-word "語")` を書いて、その見出しの候補から外す。
    /// ユーザー辞書が変わらなければ false を返す。
    pub fn purge(
        &mut self,
        midashi: &Midashi,
        word: &str,
        shared: &Dictionary,
    ) -> io::Result<bool> {
        let key = midashi.key();
        let in_user = self.lookup(&key).any(|w| w == word);
        let purge = match shared.written(&key, word) {
            Some(written) => Purge::Ignore(written),
            None if in_user => Purge::Delete,
            None => Purge::Ignore(word),
        };
        let mut changed = false;
        self.rewrite(|text| {
            let purged = rinze_dict::purge_entry(text, section(midashi), &key, word, purge);
            changed = purged.is_some();
            Ok(purged.unwrap_or_else(|| text.to_string()))
        })?;
        Ok(changed)
    }

    fn rewrite(&mut self, edit: impl FnOnce(&str) -> io::Result<String>) -> io::Result<()> {
        let text = match &mut self.storage {
            Storage::Memory(text) => {
                *text = edit(text)?;
                text.clone()
            }
            Storage::File(path) => file::rewrite(path, edit)?,
        };
        self.dict = Dictionary::parse(&text);
        Ok(())
    }
}

fn section(midashi: &Midashi) -> Section {
    match midashi.okuri() {
        Some(_) => Section::OkuriAri,
        None => Section::OkuriNasi,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rinze_engine::Okuri;

    use super::*;

    fn kana(reading: &str) -> Midashi {
        Midashi::Kana {
            reading: reading.into(),
            okuri: None,
        }
    }

    fn okuri_ari(reading: &str, consonant: char, okuri: &str) -> Midashi {
        Midashi::Kana {
            reading: reading.into(),
            okuri: Some(Okuri {
                consonant,
                kana: okuri.into(),
            }),
        }
    }

    #[test]
    fn 登録した語はファイルに残り_開き直しても引ける() {
        let dir = std::env::temp_dir().join(format!("rinze-user-dict-{}", std::process::id()));
        let path = dir.join("nested").join("user.jisyo");
        let mut user = UserDictionary::open(path.clone()).unwrap();
        user.register(&kana("まよねーず"), "マヨネーズ").unwrap();
        user.register(&okuri_ari("あ", 'e', "え"), "和").unwrap();
        user.register(&Midashi::Abbrev("file".into()), "ファイル")
            .unwrap();
        assert_eq!(user.lookup("あe").collect::<Vec<_>>(), ["和"]);
        assert!(user.register(&kana("あ"), "a/b").is_err());

        let reopened = UserDictionary::open(path.clone()).unwrap();
        assert_eq!(
            reopened.lookup("まよねーず").collect::<Vec<_>>(),
            ["マヨネーズ"]
        );
        assert_eq!(reopened.lookup("あe").collect::<Vec<_>>(), ["和"]);
        assert_eq!(reopened.lookup("file").collect::<Vec<_>>(), ["ファイル"]);
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.ends_with(";; okuri-nasi entries.\nfile /ファイル/\nまよねーず /マヨネーズ/\n"),
            "英字の見出しは送りなしの節に書く: {text}"
        );
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            !names.iter().any(|n| n.contains(".tmp.")),
            "一時ファイルを残さない: {names:?}"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ユーザー辞書だけの語は行から除き_ファイルにも残さない() {
        let dir =
            std::env::temp_dir().join(format!("rinze-user-dict-purge-{}", std::process::id()));
        let path = dir.join("user.jisyo");
        let mut user = UserDictionary::open(path.clone()).unwrap();
        user.register(&kana("りんぜ"), "凛是").unwrap();
        user.register(&kana("りんぜ"), "林是").unwrap();
        assert!(
            user.purge(&kana("りんぜ"), "凛是", &Dictionary::default())
                .unwrap()
        );
        assert_eq!(user.lookup("りんぜ").collect::<Vec<_>>(), ["林是"]);
        let reopened = UserDictionary::open(path.clone()).unwrap();
        assert_eq!(reopened.lookup("りんぜ").collect::<Vec<_>>(), ["林是"]);
        assert!(reopened.ignored("りんぜ").is_empty());
        let shared = Dictionary::parse("あした /明日/\n");
        assert!(user.purge(&kana("あした"), "明日", &shared).unwrap());
        assert!(
            !user.purge(&kana("あした"), "明日", &shared).unwrap(),
            "既に外した語をもう一度消しても何も変えない"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn 別の書き手が足した語を消さない() {
        let dir =
            std::env::temp_dir().join(format!("rinze-user-dict-merge-{}", std::process::id()));
        let path = dir.join("user.jisyo");
        let mut a = UserDictionary::open(path.clone()).unwrap();
        let mut b = UserDictionary::open(path.clone()).unwrap();
        a.register(&kana("りんぜ"), "凛是").unwrap();
        b.register(&kana("まよ"), "真夜").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("りんぜ /凛是/") && text.contains("まよ /真夜/"),
            "{text}"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
