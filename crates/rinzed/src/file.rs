//! rinzed だけが書くテキストファイル (ユーザー辞書と学習の履歴)。
//!
//! 別のソケットで動く rinzed が同じファイルを書くことがあるので、書き換えはロックを取ってから最新の内容を読み直して行う。
//! 書くときは一時ファイルに書いて fsync してから rename するので、途中で落ちても壊れない。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// ファイルの最新の内容を `edit` で書き換え、書き換えたあとの内容を返す。内容が変わらなければ書かない。
pub fn rewrite(path: &Path, edit: impl FnOnce(&str) -> io::Result<String>) -> io::Result<String> {
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
    let latest = read_or_empty(path)?;
    let text = edit(&latest)?;
    if text != latest {
        write_atomically(path, text.as_bytes())?;
    }
    Ok(text)
}

/// ファイルが無ければ空として読む。
pub fn read_or_empty(path: &Path) -> io::Result<String> {
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
