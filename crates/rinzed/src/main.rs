use std::convert::Infallible;
use std::ffi::OsString;
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;

use rinze_zenz::{Model, Zenz};
use rinzed::history::History;
use rinzed::user_dict::UserDictionary;
use rinzed::{Backend, Learning, log};

const USAGE: &str = "使い方: rinzed [--model <zenz の gguf>] [--dict <SKK 辞書>]... [--socket <パス>] [--user-dict <パス>] [--history <パス> | --no-learning] [--threads <数>]
  --model を省くと環境変数 RINZED_DEFAULT_MODEL のモデルを、--dict を 1 つも渡さないと RINZED_DEFAULT_DICTS (: 区切り) の辞書を使う";

const DEFAULT_MODEL_ENV: &str = "RINZED_DEFAULT_MODEL";
const DEFAULT_DICTS_ENV: &str = "RINZED_DEFAULT_DICTS";

/// `sockaddr_un.sun_path` の大きさ。パスは終端の NUL を含めてここに収まらなければならない。
#[cfg(target_os = "macos")]
const SUN_PATH_LEN: usize = 104;
#[cfg(not(target_os = "macos"))]
const SUN_PATH_LEN: usize = 108;

const DEFAULT_THREADS_WHEN_UNKNOWN: usize = 4;
const DEFAULT_THREADS_CAP: usize = 8;

#[derive(Debug, PartialEq)]
struct Args {
    model: PathBuf,
    dicts: Vec<PathBuf>,
    socket: PathBuf,
    user_dict: PathBuf,
    learning: LearningArg,
    threads: i32,
}

#[derive(Debug, PartialEq)]
enum LearningArg {
    Off,
    History(PathBuf),
}

fn parse_args(
    args: impl IntoIterator<Item = String>,
    env: impl Fn(&str) -> Option<OsString>,
) -> Result<Args, String> {
    let env = |name: &str| env(name).filter(|value| !value.is_empty());
    let mut model = None;
    let mut dicts = Vec::new();
    let mut socket = None;
    let mut user_dict = None;
    let mut history = None;
    let mut no_learning = false;
    let mut threads = None;
    let mut it = args.into_iter();
    while let Some(flag) = it.next() {
        if flag == "--no-learning" {
            no_learning = true;
            continue;
        }
        let value = it.next().ok_or_else(|| format!("{flag} に値がない"))?;
        match flag.as_str() {
            "--model" => model = Some(PathBuf::from(value)),
            "--dict" => dicts.push(PathBuf::from(value)),
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
    if dicts.is_empty() {
        dicts = env(DEFAULT_DICTS_ENV)
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_default();
    }
    if dicts.is_empty() {
        return Err(format!("--dict が必要 ({DEFAULT_DICTS_ENV} も無い)"));
    }
    Ok(Args {
        model: model
            .or_else(|| env(DEFAULT_MODEL_ENV).map(PathBuf::from))
            .ok_or_else(|| format!("--model が必要 ({DEFAULT_MODEL_ENV} も無い)"))?,
        dicts,
        socket: socket.map_or_else(rinze_socket::default_socket, Ok)?,
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
    let args = match parse_args(std::env::args().skip(1), |name| std::env::var_os(name)) {
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

    let dict = rinzed::load_dictionaries(&args.dicts)?;
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

    let dicts: Vec<String> = args.dicts.iter().map(|d| d.display().to_string()).collect();
    let note = format!(
        "モデル {}、辞書 {}、ユーザー辞書 {}、{learning_note}",
        args.model.display(),
        dicts.join(" "),
        args.user_dict.display()
    );
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
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{Args, LearningArg, parse_args};

    fn parse(args: &[&str], env: &[(&str, &str)]) -> Result<Args, String> {
        let fixed = ["--socket", "/s", "--user-dict", "/u", "--no-learning"];
        parse_args(args.iter().chain(&fixed).map(|a| a.to_string()), |name| {
            env.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        })
    }

    #[test]
    fn 辞書は渡した順に複数受け取る() {
        let args = parse(
            &["--model", "/m.gguf", "--dict", "/L", "--dict", "/jinmei"],
            &[],
        )
        .unwrap();
        assert_eq!(args.model, PathBuf::from("/m.gguf"));
        assert_eq!(args.dicts, [PathBuf::from("/L"), PathBuf::from("/jinmei")]);
        assert_eq!(args.learning, LearningArg::Off);
    }

    #[test]
    fn モデルと辞書を省くと環境変数の既定値を使い_引数があれば引数を使う() {
        let env = [
            ("RINZED_DEFAULT_MODEL", "/store/zenz.gguf"),
            ("RINZED_DEFAULT_DICTS", "/store/L:/store/geo"),
        ];
        let defaults = parse(&[], &env).unwrap();
        assert_eq!(defaults.model, PathBuf::from("/store/zenz.gguf"));
        assert_eq!(
            defaults.dicts,
            [PathBuf::from("/store/L"), PathBuf::from("/store/geo")]
        );
        let given = parse(&["--model", "/m.gguf", "--dict", "/mine"], &env).unwrap();
        assert_eq!(given.model, PathBuf::from("/m.gguf"));
        assert_eq!(
            given.dicts,
            [PathBuf::from("/mine")],
            "--dict を渡せば既定の辞書は使わない"
        );
    }

    #[test]
    fn モデルか辞書が引数にも環境変数にも無ければ起動しない() {
        assert!(parse(&["--dict", "/L"], &[]).is_err());
        assert!(parse(&["--model", "/m.gguf"], &[("RINZED_DEFAULT_DICTS", "")]).is_err());
    }
}
