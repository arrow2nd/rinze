//! rinzed が待ち受け、フロントエンドが接続する既定のソケットのパス、所有者の検証、msgpack フレームの境界検査。
//! rinzed と rinze-client はこの規則を共有する。
//! Neovim のプラグインは Lua で同じ規則を持ち、`tests/fixtures/default_socket.json` の表で一致を確かめる。

pub mod frame;

use std::ffi::OsString;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Other,
}

pub const CURRENT_OS: Os = if cfg!(target_os = "macos") {
    Os::MacOs
} else {
    Os::Other
};

pub fn default_socket() -> Result<PathBuf, String> {
    socket_path(CURRENT_OS, |name| std::env::var_os(name))
}

/// macOS の `$TMPDIR` は nix develop などで変わり、同じ利用者のプロセスどうしで一致しないことがあるので使わない。
/// Linux で `$XDG_RUNTIME_DIR` が無ければ共有の `/tmp` に落とさず、`$XDG_CACHE_HOME` または `$HOME/.cache` の下を使う。
pub fn socket_path(os: Os, env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    let env = |name: &str| env(name).filter(|value| !value.is_empty());
    if let Os::MacOs = os {
        let home = env("HOME").ok_or("HOME が無いので既定のパスを決められない")?;
        return Ok(PathBuf::from(home).join("Library/Caches/rinze/rinze.sock"));
    }
    if let Some(dir) = env("XDG_RUNTIME_DIR") {
        return Ok(PathBuf::from(dir).join("rinze.sock"));
    }
    let cache = match env("XDG_CACHE_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => {
            let home = env("HOME").ok_or("HOME が無いので既定のパスを決められない")?;
            PathBuf::from(home).join(".cache")
        }
    };
    Ok(cache.join("rinze/rinze.sock"))
}

pub fn current_uid() -> u32 {
    // POSIX の geteuid は常に成功する
    unsafe { libc::geteuid() }
}

/// ソケットのパスがシンボリックリンクでなく、自分の所有する Unix ドメインソケットであることを確かめる。
pub fn verify_socket_owner(path: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.file_type().is_socket() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} は Unix ドメインソケットでない", path.display()),
        ));
    }
    let my_uid = current_uid();
    if meta.uid() != my_uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "ソケット {} の所有者 (uid {}) が自分 (uid {my_uid}) と異なる",
                path.display(),
                meta.uid()
            ),
        ));
    }
    Ok(())
}

/// 接続相手のプロセスの実効 uid が自分と同じであることを確かめる。
pub fn verify_peer_uid(stream: &UnixStream) -> io::Result<()> {
    let peer_uid = peer_uid(stream)?;
    let my_uid = current_uid();
    if peer_uid != my_uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("接続先の所有者 (uid {peer_uid}) が自分 (uid {my_uid}) と異なる"),
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ret = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(cred.uid)
}

#[cfg(any(
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
))]
fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let mut euid: libc::uid_t = 0;
    let mut egid: libc::gid_t = 0;
    let ret = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut euid, &mut egid) };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(euid)
}

/// ソケットの所有者と接続相手の uid を確かめて接続する。
pub fn connect_verified(path: &Path) -> io::Result<UnixStream> {
    verify_socket_owner(path)?;
    let stream = UnixStream::connect(path)?;
    verify_peer_uid(&stream)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;

    use super::{Os, connect_verified, socket_path, verify_peer_uid, verify_socket_owner};

    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        os: String,
        env: HashMap<String, String>,
        socket: Option<PathBuf>,
    }

    #[test]
    fn default_socket_matches_shared_table() {
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../tests/fixtures/default_socket.json")).unwrap();
        let mut failures = Vec::new();
        for case in &cases {
            let os = match case.os.as_str() {
                "macos" => Os::MacOs,
                "linux" => Os::Other,
                other => panic!("{}: 未知の os {other}", case.name),
            };
            let got = socket_path(os, |name| case.env.get(name).map(OsString::from)).ok();
            if got != case.socket {
                failures.push(format!("{}: {got:?} (期待 {:?})", case.name, case.socket));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn 自分のソケットと接続相手は検証を通り_通常ファイルやシンボリックリンクは拒む() {
        let dir = std::env::temp_dir().join(format!("rinze-socket-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let sock = dir.join("r.sock");
        let _listener = UnixListener::bind(&sock).unwrap();
        verify_socket_owner(&sock).unwrap();
        let stream = connect_verified(&sock).unwrap();
        verify_peer_uid(&stream).unwrap();

        let link = dir.join("link.sock");
        std::os::unix::fs::symlink(&sock, &link).unwrap();
        let err = verify_socket_owner(&link).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);

        let file = dir.join("plain");
        std::fs::write(&file, b"").unwrap();
        let err = verify_socket_owner(&file).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);

        let (a, b) = UnixStream::pair().unwrap();
        verify_peer_uid(&a).unwrap();
        verify_peer_uid(&b).unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
