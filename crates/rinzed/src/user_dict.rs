//! ユーザー辞書。SKK 辞書と同じ書式の UTF-8 のテキストで、書き込むのは rinzed だけ。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rinze_dict::{Dictionary, Section};
use rinze_engine::Midashi;

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
        let text = read_or_empty(&path)?;
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

    pub fn lookup(&self, key: &str) -> &[String] {
        self.dict.lookup(key)
    }

    /// 語が辞書の書式で書けなければ何もせずにエラーを返す。
    pub fn register(&mut self, midashi: &Midashi, word: &str) -> io::Result<()> {
        let section = match midashi.okuri() {
            Some(_) => Section::OkuriAri,
            None => Section::OkuriNasi,
        };
        self.rewrite(|text| {
            rinze_dict::add_entry(text, section, &midashi.key(), word).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("「{word}」は SKK 辞書の候補として書けない"),
                )
            })
        })
    }

    /// 語がユーザー辞書に無ければ何もせずに false を返す。
    pub fn purge(&mut self, midashi: &Midashi, word: &str) -> io::Result<bool> {
        let mut found = false;
        self.rewrite(|text| {
            let removed = rinze_dict::remove_entry(text, &midashi.key(), word);
            found = removed.is_some();
            Ok(removed.unwrap_or_else(|| text.to_string()))
        })?;
        Ok(found)
    }

    fn rewrite(&mut self, edit: impl FnOnce(&str) -> io::Result<String>) -> io::Result<()> {
        let text = match &mut self.storage {
            Storage::Memory(text) => {
                *text = edit(text)?;
                text.clone()
            }
            Storage::File(path) => {
                let (_lock, latest) = lock_and_read(path)?;
                let text = edit(&latest)?;
                if text != latest {
                    write_atomically(path, text.as_bytes())?;
                }
                text
            }
        };
        self.dict = Dictionary::parse(&text);
        Ok(())
    }
}

fn lock_and_read(path: &Path) -> io::Result<(File, String)> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // ファイル本体は rename で差し替わるので、ロックは別のファイルで取る
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(with_suffix(path, ".lock"))?;
    lock.lock()?;
    let text = read_or_empty(path)?;
    Ok((lock, text))
}

fn read_or_empty(path: &Path) -> io::Result<String> {
    match fs::read(path) {
        Ok(bytes) => Ok(rinze_dict::decode(&bytes).into_owned()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = with_suffix(path, &format!(".tmp.{}", std::process::id()));
    let mut file = File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        // rename 自体を永続化する
        File::open(dir)?.sync_all()?;
    }
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(user.lookup("あe"), ["和"]);
        assert!(user.register(&kana("あ"), "a/b").is_err());

        let reopened = UserDictionary::open(path.clone()).unwrap();
        assert_eq!(reopened.lookup("まよねーず"), ["マヨネーズ"]);
        assert_eq!(reopened.lookup("あe"), ["和"]);
        assert_eq!(reopened.lookup("file"), ["ファイル"]);
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
    fn 消した語はファイルからも消え_無い語を消しても何も変えない() {
        let dir =
            std::env::temp_dir().join(format!("rinze-user-dict-purge-{}", std::process::id()));
        let path = dir.join("user.jisyo");
        let mut user = UserDictionary::open(path.clone()).unwrap();
        user.register(&kana("りんぜ"), "凛是").unwrap();
        user.register(&kana("りんぜ"), "林是").unwrap();
        assert!(user.purge(&kana("りんぜ"), "凛是").unwrap());
        assert_eq!(user.lookup("りんぜ"), ["林是"]);
        assert!(!user.purge(&kana("りんぜ"), "凛是").unwrap());
        let reopened = UserDictionary::open(path.clone()).unwrap();
        assert_eq!(reopened.lookup("りんぜ"), ["林是"]);
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
