use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const TEMP_ATTEMPTS: usize = 64;
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) fn rewrite(
    path: &Path,
    edit: impl FnOnce(&str) -> io::Result<String>,
) -> io::Result<String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // ファイル本体は rename で差し替わるので、ロックは別のファイルで取る
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(with_suffix(path, ".lock"))?;
    let my_uid = rinze_socket::current_uid();
    let uid = lock.metadata()?.uid();
    if uid != my_uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("ロックファイルの所有者 (uid {uid}) が自分 (uid {my_uid}) と異なる"),
        ));
    }
    lock.lock()?;
    let latest = read_or_empty(path)?;
    let text = edit(&latest)?;
    if text != latest {
        write_atomically(path, text.as_bytes())?;
    }
    Ok(text)
}

pub(crate) fn read_or_empty(path: &Path) -> io::Result<String> {
    match fs::read(path) {
        Ok(bytes) => Ok(rinze_dict::decode(&bytes).into_owned()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

fn create_temp(path: &Path) -> io::Result<(PathBuf, File)> {
    let pid = std::process::id();
    for _ in 0..TEMP_ATTEMPTS {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp = with_suffix(path, &format!(".tmp.{pid}.{seq}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
        {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "一時ファイルを作れない",
    ))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (tmp, mut file) = create_temp(path)?;
    let write_result = file.write_all(bytes).and_then(|()| file.sync_all());
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
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
