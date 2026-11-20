use std::convert::Infallible;
use std::ffi::OsString;
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;

use rinze_dict::Dictionary;
use rinze_zenz::{Model, Zenz};
use rinzed::history::History;
use rinzed::user_dict::UserDictionary;
use rinzed::{Backend, Learning, log};

const USAGE: &str = "使い方: rinzed --model <zenz の gguf> --dict <SKK 辞書> [--socket <パス>] [--user-dict <パス>] [--history <パス> | --no-learning] [--threads <数>]";

/// `sockaddr_un.sun_path` の大きさ。パスは終端の NUL を含めてここに収まらなければならない。
#[cfg(target_os = "macos")]
const SUN_PATH_LEN: usize = 104;
#[cfg(not(target_os = "macos"))]
const SUN_PATH_LEN: usize = 108;

const DEFAULT_THREADS_WHEN_UNKNOWN: usize = 4;
const DEFAULT_THREADS_CAP: usize = 8;

struct Args {
    model: PathBuf,
    dict: PathBuf,
    socket: PathBuf,
    user_dict: PathBuf,
    learning: LearningArg,
    threads: i32,
}

enum LearningArg {
    Off,
    History(PathBuf),
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut dict = None;
    let mut socket = None;
    let mut user_dict = None;
    let mut history = None;
    let mut no_learning = false;
    let mut threads = None;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if flag == "--no-learning" {
            no_learning = true;
            continue;
        }
        let value = it.next().ok_or_else(|| format!("{flag} に値がない"))?;
        match flag.as_str() {
            "--model" => model = Some(PathBuf::from(value)),
            "--dict" => dict = Some(PathBuf::from(value)),
            "--socket" => socket = Some(PathBuf::from(value)),
            "--user-dict" => user_dict = Some(PathBuf::from(value)),
            "--history" => history = Some(PathBuf::from(value)),
            "--threads" => {
                threads = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--threads が数でない: {value}"))?,
                )
            }
            _ => return Err(format!("未知の引数: {flag}")),
        }
    }
    Ok(Args {
        model: model.ok_or("--model が必要")?,
        dict: dict.ok_or("--dict が必要")?,
        socket: socket.map_or_else(default_socket, Ok)?,
        user_dict: user_dict.map_or_else(default_user_dict, Ok)?,
        learning: match (no_learning, history) {
            (true, Some(_)) => return Err("--no-learning と --history は同時に指定できない".into()),
            (true, None) => LearningArg::Off,
            (false, history) => LearningArg::History(history.map_or_else(default_history, Ok)?),
        },
        threads: threads.unwrap_or_else(default_threads),
    })
}

fn default_threads() -> i32 {
    std::thread::available_parallelism().map_or(DEFAULT_THREADS_WHEN_UNKNOWN, |n| {
        n.get().min(DEFAULT_THREADS_CAP)
    }) as i32
}

fn home(home: Option<OsString>) -> Result<PathBuf, String> {
    home.map(PathBuf::from)
        .ok_or_else(|| "HOME が無いので既定のパスを決められない".into())
}

#[derive(Clone, Copy)]
enum Os {
    MacOs,
    Other,
}

const CURRENT_OS: Os = if cfg!(target_os = "macos") {
    Os::MacOs
} else {
    Os::Other
};

fn default_socket() -> Result<PathBuf, String> {
    socket_path(CURRENT_OS, |name| std::env::var_os(name))
}

/// Neovim プラグイン (`nvim/lua/rinze/socket.lua`) も同じ規則でパスを決める。
/// 両者の一致は `tests/fixtures/default_socket.json` の表で確かめる。
///
/// macOS の `$TMPDIR` は nix develop などで変わり、同じ利用者のプロセスどうしで一致しないことがあるので使わない。
fn socket_path(os: Os, env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    let env = |name: &str| env(name).filter(|value| !value.is_empty());
    if let Os::MacOs = os {
        return Ok(home(env("HOME"))?.join("Library/Caches/rinze/rinze.sock"));
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

fn default_user_dict() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(
            home(std::env::var_os("HOME"))?.join("Library/Application Support/rinze/user.jisyo")
        );
    }
    let data = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => home(std::env::var_os("HOME"))?.join(".local/share"),
    };
    Ok(data.join("rinze/user.jisyo"))
}

/// 履歴は消えても困らないが残したい状態なので、Linux では XDG の state に置く。macOS には state に当たる場所が無い。
fn default_history() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(
            home(std::env::var_os("HOME"))?.join("Library/Application Support/rinze/history.tsv")
        );
    }
    let state = match std::env::var_os("XDG_STATE_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home(std::env::var_os("HOME"))?.join(".local/state"),
    };
    Ok(state.join("rinze/history.tsv"))
}

fn check_socket_path(socket: &Path) -> Result<(), String> {
    let len = socket.as_os_str().len();
    if len < SUN_PATH_LEN {
        return Ok(());
    }
    Err(format!(
        "ソケットのパス {} は {len} バイトあり、Unix ドメインソケットのパスの上限 ({} バイト) を超える。\
         --socket (Neovim では setup の socket) で短いパスを指定する",
        socket.display(),
        SUN_PATH_LEN - 1
    ))
}

/// ロックは rinzed が落ちれば OS が外すので、ロックを取れた時点で残っているソケットは前の rinzed の残骸と分かる。
/// ロックファイルは消さない。消すと、消す前のファイルと作り直したファイルで別々にロックを取れてしまう。
fn lock_socket(socket: &Path) -> Result<Option<File>, String> {
    let mut path = socket.as_os_str().to_owned();
    path.push(".lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| {
            format!(
                "ロックファイル {} を開けない: {e}",
                Path::new(&path).display()
            )
        })?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(format!("{} をロックできない: {e}", socket.display())),
    }
}

struct AnotherRinzedServes;

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match start(args) {
        Ok(AnotherRinzedServes) => ExitCode::SUCCESS,
        Err(e) => {
            log!("rinzed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn start(args: Args) -> Result<AnotherRinzedServes, Box<dyn std::error::Error>> {
    check_socket_path(&args.socket)?;
    if let Some(dir) = args.socket.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("ソケットのディレクトリ {} を作れない: {e}", dir.display()))?;
    }
    let Some(_lock) = lock_socket(&args.socket)? else {
        log!(
            "rinzed: {} は別の rinzed が使っている",
            args.socket.display()
        );
        return Ok(AnotherRinzedServes);
    };

    let dict = Dictionary::from_bytes(
        &std::fs::read(&args.dict)
            .map_err(|e| format!("辞書 {} を読めない: {e}", args.dict.display()))?,
    );
    let user = UserDictionary::open(args.user_dict.clone())
        .map_err(|e| format!("ユーザー辞書 {} を読めない: {e}", args.user_dict.display()))?;
    let (learning, learning_note) = match &args.learning {
        LearningArg::Off => (Learning::Off, "学習なし".to_string()),
        LearningArg::History(path) => {
            let history = History::open(path.clone())
                .map_err(|e| format!("学習の履歴 {} を読めない: {e}", path.display()))?;
            (
                Learning::On(Mutex::new(history)),
                format!("学習の履歴 {}", path.display()),
            )
        }
    };
    let model_error = |e| format!("モデル {} を読めない: {e}", args.model.display());
    let model: &'static Model = Box::leak(Box::new(Model::load(&args.model).map_err(model_error)?));
    let zenz = Zenz::new(model, args.threads).map_err(model_error)?;
    let backend: &'static Backend<'static> =
        Box::leak(Box::new(Backend::new(dict, user, zenz, learning)));

    let note = format!("ユーザー辞書 {}、{learning_note}", args.user_dict.display());
    match serve_on(&args.socket, &note, backend)? {}
}

fn serve_on(
    socket: &Path,
    note: &str,
    backend: &'static Backend<'static>,
) -> Result<Infallible, Box<dyn std::error::Error>> {
    match std::fs::remove_file(socket) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("残っていたソケット {} を消せない: {e}", socket.display()).into());
        }
        _ => {}
    }
    let listener = UnixListener::bind(socket)?;
    log!("rinzed: {} で待ち受け中 ({note})", socket.display());
    loop {
        let (stream, _) = listener.accept()?;
        std::thread::spawn(move || {
            if let Err(e) = rinzed::serve(stream, backend) {
                log!("rinzed: 接続を閉じた: {e}");
            }
        });
    }
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
