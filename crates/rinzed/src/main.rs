use std::convert::Infallible;
use std::ffi::OsString;
use std::fs::{DirBuilder, File, OpenOptions, Permissions, TryLockError};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rinze_zenz::{Model, Zenz};
use rinzed::history::History;
use rinzed::user_dict::UserDictionary;
use rinzed::{Backend, Learning, log};

const USAGE: &str = "使い方: rinzed [--model <zenz の gguf>] [--dict <SKK 辞書>]... [--socket <パス>] [--user-dict <パス>] [--history <パス> | --no-learning] [--threads <数>] [--verbose]
        rinzed --version
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
const MAX_CONNECTIONS: usize = 64;
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

#[derive(Debug, PartialEq)]
enum Action {
    Run(Args),
    Version,
}

#[derive(Debug, PartialEq)]
struct Args {
    model: PathBuf,
    dicts: Vec<PathBuf>,
    socket: PathBuf,
    user_dict: PathBuf,
    learning: LearningArg,
    threads: i32,
    verbose: bool,
}

#[derive(Debug, PartialEq)]
enum LearningArg {
    Off,
    History(PathBuf),
}

fn parse_args(
    args: impl IntoIterator<Item = String>,
    env: impl Fn(&str) -> Option<OsString>,
) -> Result<Action, String> {
    let env = |name: &str| env(name).filter(|value| !value.is_empty());
    let mut model = None;
    let mut dicts = Vec::new();
    let mut socket = None;
    let mut user_dict = None;
    let mut history = None;
    let mut no_learning = false;
    let mut threads = None;
    let mut verbose = false;
    let mut it = args.into_iter();
    while let Some(flag) = it.next() {
        if flag == "--version" || flag == "-V" {
            return Ok(Action::Version);
        }
        if flag == "--no-learning" {
            no_learning = true;
            continue;
        }
        if flag == "--verbose" {
            verbose = true;
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
    Ok(Action::Run(Args {
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
        verbose,
    }))
}

fn default_threads() -> i32 {
    performance_cores()
        .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
        .map_or(DEFAULT_THREADS_WHEN_UNKNOWN, |n| n.min(DEFAULT_THREADS_CAP)) as i32
}

/// Apple Silicon の高性能コアの数。高効率コアにもスレッドを置くと、遅いコアを待つぶん点数づけが遅くなる。
#[cfg(target_os = "macos")]
fn performance_cores() -> Option<usize> {
    let mut count: libc::c_int = 0;
    let mut size = size_of::<libc::c_int>();
    // SAFETY: 名前は NUL 終端で、count と size は呼び出しの間だけ使う c_int 1 つぶんの領域を指す
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.perflevel0.physicalcpu".as_ptr(),
            (&raw mut count).cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && count > 0).then_some(count as usize)
}

#[cfg(not(target_os = "macos"))]
fn performance_cores() -> Option<usize> {
    None
}

fn home(home: Option<OsString>) -> Result<PathBuf, String> {
    home.filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME が無いので既定のパスを決められない".into())
}

fn default_user_dict() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(
            home(std::env::var_os("HOME"))?.join("Library/Application Support/rinze/user.jisyo")
        );
    }
    let data = match std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
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
    let lock_path = Path::new(&path);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock_path)
        .map_err(|e| format!("ロックファイル {} を開けない: {e}", lock_path.display()))?;
    let meta = file.metadata().map_err(|e| {
        format!(
            "ロックファイル {} の情報を読めない: {e}",
            lock_path.display()
        )
    })?;
    let my_uid = rinze_socket::current_uid();
    if meta.uid() != my_uid {
        return Err(format!(
            "ロックファイル {} の所有者 (uid {}) が自分 (uid {my_uid}) と異なる",
            lock_path.display(),
            meta.uid()
        ));
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(format!("{} をロックできない: {e}", socket.display())),
    }
}

struct AnotherRinzedServes;

// libgomp の既定では待つスレッドが CPU を回し続け、CPU が詰まっているときの点数づけが何十倍も遅くなる。
// 待ち方は libgomp が起動時に読む環境変数でしか決められないので、決まっていなければ決めて実行し直す
#[cfg(target_os = "linux")]
fn wait_passively_in_openmp() {
    use std::os::unix::process::CommandExt;
    const OMP_WAIT_POLICY: &str = "OMP_WAIT_POLICY";
    if std::env::var_os(OMP_WAIT_POLICY).is_some() {
        return;
    }
    let mut args = std::env::args_os();
    let e = std::process::Command::new("/proc/self/exe")
        .arg0(args.next().unwrap_or_default())
        .args(args)
        .env(OMP_WAIT_POLICY, "PASSIVE")
        .exec();
    log!("rinzed: {OMP_WAIT_POLICY} を決めて実行し直せない: {e}");
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1), |name| std::env::var_os(name)) {
        Ok(Action::Version) => {
            println!("rinzed {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(Action::Run(a)) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    #[cfg(target_os = "linux")]
    wait_passively_in_openmp();
    match start(args) {
        Ok(AnotherRinzedServes) => ExitCode::SUCCESS,
        Err(e) => {
            log!("rinzed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn start(args: Args) -> Result<AnotherRinzedServes, Box<dyn std::error::Error>> {
    rinzed::set_verbose(args.verbose);
    check_socket_path(&args.socket)?;
    if let Some(dir) = args.socket.parent().filter(|d| !d.as_os_str().is_empty())
        && !dir.exists()
    {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
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
    // モデルを読めなくても、辞書と学習の履歴だけで変換できるように起動する
    let zenz = Model::load(&args.model)
        .and_then(|model| {
            let model: &'static Model = Box::leak(Box::new(model));
            Zenz::new(model, args.threads)
        })
        .inspect_err(|e| {
            log!(
                "rinzed: モデル {} を読めないので zenz なしで動く: {e}",
                args.model.display()
            );
        })
        .ok();
    let model_note = if zenz.is_some() {
        ""
    } else {
        " (読めない)"
    };
    let backend: &'static Backend = Box::leak(Box::new(Backend::new(dict, user, zenz, learning)));

    let dicts: Vec<String> = args.dicts.iter().map(|d| d.display().to_string()).collect();
    let note = format!(
        "モデル {}{model_note}、辞書 {}、ユーザー辞書 {}、{learning_note}",
        args.model.display(),
        dicts.join(" "),
        args.user_dict.display()
    );
    match serve_on(&args.socket, &note, backend)? {}
}

struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn try_acquire(active: &Arc<AtomicUsize>, limit: usize) -> Option<ConnectionGuard> {
    active
        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < limit).then_some(n + 1)
        })
        .ok()
        .map(|_| ConnectionGuard(Arc::clone(active)))
}

fn serve_on(
    socket: &Path,
    note: &str,
    backend: &'static Backend,
) -> Result<Infallible, Box<dyn std::error::Error>> {
    match std::fs::symlink_metadata(socket) {
        Ok(meta) => {
            if !meta.file_type().is_socket() {
                return Err(format!("{} はソケットでないため消せない", socket.display()).into());
            }
            let my_uid = rinze_socket::current_uid();
            if meta.uid() != my_uid {
                return Err(format!(
                    "残っていたソケット {} の所有者 (uid {}) が自分 (uid {my_uid}) と異なる",
                    socket.display(),
                    meta.uid()
                )
                .into());
            }
            std::fs::remove_file(socket)
                .map_err(|e| format!("残っていたソケット {} を消せない: {e}", socket.display()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "残っていたソケット {} を確かめられない: {e}",
                socket.display()
            )
            .into());
        }
    }
    let listener = UnixListener::bind(socket)?;
    std::fs::set_permissions(socket, Permissions::from_mode(0o600))?;
    let active = Arc::new(AtomicUsize::new(0));
    log!("rinzed: {} で待ち受け中 ({note})", socket.display());
    loop {
        let (stream, _) = match listener.accept() {
            Ok(accepted) => accepted,
            Err(e) => {
                log!("rinzed: 接続を受け付けられない: {e}");
                std::thread::sleep(ACCEPT_BACKOFF);
                continue;
            }
        };
        if let Err(e) = rinze_socket::verify_peer_uid(&stream) {
            log!("rinzed: 接続を拒んだ: {e}");
            continue;
        }
        let Some(guard) = try_acquire(&active, MAX_CONNECTIONS) else {
            log!("rinzed: 同時接続数の上限 ({MAX_CONNECTIONS}) に達したので接続を閉じた");
            continue;
        };
        if let Err(e) = std::thread::Builder::new().spawn(move || {
            let _guard = guard;
            if let Err(e) = rinzed::serve(stream, backend) {
                log!("rinzed: 接続を閉じた: {e}");
            }
        }) {
            log!("rinzed: 接続のスレッドを作れない: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{Action, Args, DEFAULT_THREADS_CAP, LearningArg, default_threads, parse_args};

    #[test]
    fn 既定のスレッドの数は論理_cpu_の数と上限を超えない() {
        let logical = std::thread::available_parallelism().map_or(usize::MAX, usize::from);
        let threads = default_threads() as usize;
        assert!((1..=logical.min(DEFAULT_THREADS_CAP)).contains(&threads));
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn apple_silicon_では高性能コアの数を既定にする() {
        let cores = super::performance_cores().expect("hw.perflevel0.physicalcpu を読めない");
        assert_eq!(default_threads() as usize, cores.min(DEFAULT_THREADS_CAP));
    }

    fn parse(args: &[&str], env: &[(&str, &str)]) -> Result<Args, String> {
        let fixed = ["--socket", "/s", "--user-dict", "/u", "--no-learning"];
        match parse_args(args.iter().chain(&fixed).map(|a| a.to_string()), |name| {
            env.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        })? {
            Action::Run(a) => Ok(a),
            Action::Version => Err("unexpected Version".into()),
        }
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

    #[test]
    fn 同時接続数の上限を超えず解放すると再び受け付ける() {
        let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let g1 = super::try_acquire(&active, 2).unwrap();
        let g2 = super::try_acquire(&active, 2).unwrap();
        assert!(super::try_acquire(&active, 2).is_none());
        drop(g1);
        let g3 = super::try_acquire(&active, 2).unwrap();
        assert!(super::try_acquire(&active, 2).is_none());
        drop((g2, g3));
        assert_eq!(active.load(std::sync::atomic::Ordering::Acquire), 0);
    }

    #[test]
    fn verbose_は既定で無効で_引数で有効になる() {
        let off = parse(&["--model", "/m.gguf", "--dict", "/L"], &[]).unwrap();
        assert!(!off.verbose);
        let on = parse(&["--model", "/m.gguf", "--dict", "/L", "--verbose"], &[]).unwrap();
        assert!(on.verbose);
    }

    #[test]
    fn 空の_home_は未設定として扱う() {
        assert!(super::home(Some(OsString::from(""))).is_err());
    }

    #[test]
    fn version_フラグはモデルや辞書無しで通る() {
        assert_eq!(
            parse_args(["--version".to_string()], |_| None),
            Ok(Action::Version)
        );
        assert_eq!(
            parse_args(["-V".to_string()], |_| None),
            Ok(Action::Version)
        );
    }
}
