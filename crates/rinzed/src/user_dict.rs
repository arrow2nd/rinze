//! ユーザー辞書。SKK 辞書と同じ書式の UTF-8 のテキストで、書き込むのは rinzed だけ。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rinze_dict::Dictionary;

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

    /// `key` は送りありなら `あt` のように子音で終わる。語が辞書の書式で書けなければ何もせずにエラーを返す。
    pub fn register(&mut self, key: &str, word: &str) -> io::Result<()> {
        let text = match &mut self.storage {
            Storage::Memory(text) => {
                *text = add_entry(text, key, word)?;
                text.clone()
            }
            Storage::File(path) => {
                let (_lock, latest) = lock_and_read(path)?;
                let text = add_entry(&latest, key, word)?;
                write_atomically(path, text.as_bytes())?;
                text
            }
        };
        self.dict = Dictionary::parse(&text);
        Ok(())
    }
}

fn add_entry(text: &str, key: &str, word: &str) -> io::Result<String> {
    rinze_dict::add_entry(text, key, word).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("「{word}」は SKK 辞書の候補として書けない"),
        )
    })
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
    use super::*;

    #[test]
    fn 登録した語はファイルに残り_開き直しても引ける() {
        let dir = std::env::temp_dir().join(format!("rinze-user-dict-{}", std::process::id()));
        let path = dir.join("nested").join("user.jisyo");
        let mut user = UserDictionary::open(path.clone()).unwrap();
        user.register("まよねーず", "マヨネーズ").unwrap();
        user.register("あe", "和").unwrap();
        assert_eq!(user.lookup("あe"), ["和"]);
        assert!(user.register("あ", "a/b").is_err());

        let reopened = UserDictionary::open(path.clone()).unwrap();
        assert_eq!(reopened.lookup("まよねーず"), ["マヨネーズ"]);
        assert_eq!(reopened.lookup("あe"), ["和"]);
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
    fn 別の書き手が足した語を消さない() {
        let dir =
            std::env::temp_dir().join(format!("rinze-user-dict-merge-{}", std::process::id()));
        let path = dir.join("user.jisyo");
        let mut a = UserDictionary::open(path.clone()).unwrap();
        let mut b = UserDictionary::open(path.clone()).unwrap();
        a.register("りんぜ", "凛是").unwrap();
        b.register("まよ", "真夜").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("りんぜ /凛是/") && text.contains("まよ /真夜/"),
            "{text}"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
