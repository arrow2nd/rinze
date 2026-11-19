use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rinze_dict::Dictionary;
use rinze_zenz::Zenz;
use rinzed::user_dict::UserDictionary;
use rinzed::{Backend, log};

const USAGE: &str = "使い方: rinzed --model <zenz の gguf> --dict <SKK 辞書> [--socket <パス>] [--user-dict <パス>] [--threads <数>]";

/// `sockaddr_un.sun_path` の大きさ。パスは終端の NUL を含めてここに収まらなければならない。
#[cfg(target_os = "macos")]
const SUN_PATH_LEN: usize = 104;
#[cfg(not(target_os = "macos"))]
const SUN_PATH_LEN: usize = 108;

struct Args {
    model: PathBuf,
    dict: PathBuf,
    socket: PathBuf,
    user_dict: PathBuf,
    threads: i32,
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut dict = None;
    let mut socket = None;
    let mut user_dict = None;
    let mut threads = None;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} に値がない"))?;
        match flag.as_str() {
            "--model" => model = Some(PathBuf::from(value)),
            "--dict" => dict = Some(PathBuf::from(value)),
            "--socket" => socket = Some(PathBuf::from(value)),
            "--user-dict" => user_dict = Some(PathBuf::from(value)),
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
        threads: threads.unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(4, |n| n.get().min(8) as i32)
        }),
    })
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME が無いので既定のパスを決められない".into())
}

/// macOS の `$TMPDIR` は nix develop などで変わり、同じ利用者のプロセスどうしで一致しないことがあるので使わない。
fn default_socket() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(home()?.join("Library/Caches/rinze/rinze.sock"));
    }
    Ok(match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join("rinze.sock"),
        None => PathBuf::from(format!(
            "/tmp/rinze-{}.sock",
            std::env::var("USER").unwrap_or_default()
        )),
    })
}

fn default_user_dict() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(home()?.join("Library/Application Support/rinze/user.jisyo"));
    }
    let data = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => home()?.join(".local/share"),
    };
    Ok(data.join("rinze/user.jisyo"))
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

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match start(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log!("rinzed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn start(args: Args) -> Result<(), Box<dyn std::error::Error>> {
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
        return Ok(());
    };

    let dict = Dictionary::from_bytes(
        &std::fs::read(&args.dict)
            .map_err(|e| format!("辞書 {} を読めない: {e}", args.dict.display()))?,
    );
    let user = UserDictionary::open(args.user_dict.clone())
        .map_err(|e| format!("ユーザー辞書 {} を読めない: {e}", args.user_dict.display()))?;
    let zenz = Zenz::load(&args.model, args.threads)
        .map_err(|e| format!("モデル {} を読めない: {e}", args.model.display()))?;
    let backend: &'static Backend = Box::leak(Box::new(Backend::new(dict, user, zenz)));

    match std::fs::remove_file(&args.socket) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!(
                "残っていたソケット {} を消せない: {e}",
                args.socket.display()
            )
            .into());
        }
        _ => {}
    }
    let listener = UnixListener::bind(&args.socket)?;
    log!(
        "rinzed: {} で待ち受け中 (ユーザー辞書 {})",
        args.socket.display(),
        args.user_dict.display()
    );
    for stream in listener.incoming() {
        let stream = stream?;
        std::thread::spawn(move || {
            if let Err(e) = rinzed::serve(stream, backend) {
                log!("rinzed: 接続を閉じた: {e}");
            }
        });
    }
    Ok(())
}
