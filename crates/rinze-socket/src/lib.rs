//! rinzed が待ち受け、フロントエンドが接続する既定のソケットのパス。rinzed と rinze-client はこの規則を共有する。
//! Neovim のプラグインは Lua で同じ規則を持ち、`tests/fixtures/default_socket.json` の表で一致を確かめる。

use std::ffi::OsString;
use std::path::PathBuf;

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
pub fn socket_path(os: Os, env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    let env = |name: &str| env(name).filter(|value| !value.is_empty());
    if let Os::MacOs = os {
        let home = env("HOME").ok_or("HOME が無いので既定のパスを決められない")?;
        return Ok(PathBuf::from(home).join("Library/Caches/rinze/rinze.sock"));
    }
    Ok(match env("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join("rinze.sock"),
        None => {
            let mut path = OsString::from("/tmp/rinze-");
            path.push(env("USER").unwrap_or_default());
            path.push(".sock");
            PathBuf::from(path)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{Os, socket_path};

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
}
