use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;

use rinze_dict::Dictionary;
use rinze_zenz::Zenz;
use rinzed::Backend;

const USAGE: &str =
    "使い方: rinzed --model <zenz の gguf> --dict <SKK 辞書> [--socket <パス>] [--threads <数>]";

struct Args {
    model: PathBuf,
    dict: PathBuf,
    socket: PathBuf,
    threads: i32,
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut dict = None;
    let mut socket = None;
    let mut threads = None;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} に値がない"))?;
        match flag.as_str() {
            "--model" => model = Some(PathBuf::from(value)),
            "--dict" => dict = Some(PathBuf::from(value)),
            "--socket" => socket = Some(PathBuf::from(value)),
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
        socket: socket.unwrap_or_else(default_socket),
        // 試作では 8 スレッドで 1 回の変換が中央値約 25ms だった。それ以上は測っていない
        threads: threads.unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(4, |n| n.get().min(8) as i32)
        }),
    })
}

fn default_socket() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join("rinze.sock"),
        None => PathBuf::from(format!(
            "/tmp/rinze-{}.sock",
            std::env::var("USER").unwrap_or_default()
        )),
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
    if UnixStream::connect(&args.socket).is_ok() {
        eprintln!("rinzed: {} で既に動いている", args.socket.display());
        return ExitCode::SUCCESS;
    }
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rinzed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let dict = Dictionary::from_bytes(
        &std::fs::read(&args.dict)
            .map_err(|e| format!("辞書 {} を読めない: {e}", args.dict.display()))?,
    );
    let zenz = Zenz::load(&args.model, args.threads)
        .map_err(|e| format!("モデル {} を読めない: {e}", args.model.display()))?;
    let backend: &'static Backend = Box::leak(Box::new(Backend {
        dict,
        zenz: Mutex::new(zenz),
    }));

    // 接続できないソケットファイルは前回の常駐プロセスの残骸なので消してから bind する
    if args.socket.exists() {
        std::fs::remove_file(&args.socket)?;
    }
    // モデルを読み終えてから bind するので、接続できた時点で変換を受け付けられる
    let listener = UnixListener::bind(&args.socket)?;
    eprintln!("rinzed: {} で待ち受け中", args.socket.display());
    for stream in listener.incoming() {
        let stream = stream?;
        std::thread::spawn(move || {
            if let Err(e) = rinzed::serve(stream, backend) {
                eprintln!("rinzed: 接続を閉じた: {e}");
            }
        });
    }
    Ok(())
}
