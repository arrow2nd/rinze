//! C ABI。ヘッダ `include/rinze_client.h` は cbindgen で作り、`header_matches_ffi` のテストでずれを見つける。
//!
//! 返す文字列は UTF-8 で NUL で終わり、持ち主 (service、client、response) を次に呼ぶか解放するまで有効。
//! どの関数も panic を外に出さず、NULL を渡されても落ちない。
//!
//! 安全に呼ぶ条件はどの関数も同じで、ポインタにはこの API が返して解放していないものか NULL を、文字列には NUL で終わるものを渡す。
//! 同じ client と response を複数のスレッドから同時に使わない。
#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;

use crate::{
    Client, Error, LeftContext, Mode, Options, Origin, Response, Service, Status, Surrounding,
};

const EMPTY: &CStr = c"";

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RinzeMode {
    Hiragana,
    Katakana,
    HalfwidthKatakana,
    Ascii,
    FullwidthLatin,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RinzeStatus {
    Ready,
    Starting,
    Unavailable,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RinzeFailure {
    None,
    /// rinzed を起動中で、要求を処理していない。`rinze_service_poll` で待ち受けを始めたかを確かめる。
    Starting,
    Unavailable,
    /// 時間内に応答が無かったが、要求は rinzed に届いていて入力状態も残っている。表示はそのままにし、打鍵はアプリへ通さない。
    /// 応答は次の要求の前に読む。そのときも応答が無ければ、入力状態を捨てて Lost を返す。
    Delayed,
    /// 接続が切れたか応答を待ち切れず、rinzed 側の入力状態は失われた。
    Lost,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RinzeLeftContext {
    /// surrounding text を渡さなかったので、空の左文脈を送った。
    Unknown,
    Surrounding,
    /// surrounding text に入っていた rinze の未確定文字列を除いた。
    PreeditRemoved,
}

pub struct RinzeService {
    service: Arc<Service>,
    message: CString,
}

pub struct RinzeClient {
    client: Client,
    failure: RinzeFailure,
    message: CString,
    left_context: CString,
}

pub struct RinzeResponse {
    response: Response,
    commit: CString,
    preedit: CString,
    candidates: Vec<(CString, CString)>,
    minibuffer: Option<[CString; 3]>,
}

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn guarded<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

unsafe fn text<'a>(s: *const c_char) -> Option<std::borrow::Cow<'a, str>> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_string_lossy())
}

impl From<Mode> for RinzeMode {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Hiragana => RinzeMode::Hiragana,
            Mode::Katakana => RinzeMode::Katakana,
            Mode::HalfwidthKatakana => RinzeMode::HalfwidthKatakana,
            Mode::Ascii => RinzeMode::Ascii,
            Mode::FullwidthLatin => RinzeMode::FullwidthLatin,
        }
    }
}

impl From<RinzeMode> for Mode {
    fn from(mode: RinzeMode) -> Self {
        match mode {
            RinzeMode::Hiragana => Mode::Hiragana,
            RinzeMode::Katakana => Mode::Katakana,
            RinzeMode::HalfwidthKatakana => Mode::HalfwidthKatakana,
            RinzeMode::Ascii => Mode::Ascii,
            RinzeMode::FullwidthLatin => Mode::FullwidthLatin,
        }
    }
}

impl From<Response> for RinzeResponse {
    fn from(response: Response) -> Self {
        RinzeResponse {
            commit: c(&response.commit),
            preedit: c(&response.preedit),
            candidates: response
                .candidates
                .iter()
                .flat_map(|page| &page.items)
                .map(|(label, word)| (c(&label.to_string()), c(word)))
                .collect(),
            minibuffer: response
                .minibuffer
                .as_ref()
                .map(|m| [c(&m.prompt), c(&m.text), c(&m.preedit)]),
            response,
        }
    }
}

/// `socket` が NULL なら既定のソケット。`program` は接続できなかったときに `--socket` を付けて起動する rinzed で、NULL なら起動しない。
/// `prefetch` が true なら、見出しの読みが変わって少しの間そのままなら別の接続で候補を問い合わせ、Space での変換の待ちを隠す。
/// 既定のソケットを決められなければ NULL を返す。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_service_new(
    socket: *const c_char,
    program: *const c_char,
    prefetch: bool,
) -> *mut RinzeService {
    guarded(ptr::null_mut(), || {
        let options = Options {
            socket: unsafe { text(socket) }.map(|s| PathBuf::from(s.as_ref())),
            program: unsafe { text(program) }.map(|s| PathBuf::from(s.as_ref())),
            prefetch,
        };
        match Service::new(options) {
            Ok(service) => Box::into_raw(Box::new(RinzeService {
                service,
                message: CString::default(),
            })),
            Err(_) => ptr::null_mut(),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_service_free(service: *mut RinzeService) {
    guarded((), || {
        if !service.is_null() {
            drop(unsafe { Box::from_raw(service) });
        }
    })
}

/// rinzed の起動を待っている間に呼び、待ち受けを始めたか、起動に失敗したかを返す。待たずに戻る。
/// `message` には UNAVAILABLE のときの理由を入れる。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_service_poll(
    service: *mut RinzeService,
    message: *mut *const c_char,
) -> RinzeStatus {
    guarded(RinzeStatus::Unavailable, || {
        let Some(service) = (unsafe { service.as_mut() }) else {
            return RinzeStatus::Unavailable;
        };
        let (status, text) = match service.service.poll() {
            Status::Ready => (RinzeStatus::Ready, String::new()),
            Status::Starting => (RinzeStatus::Starting, Error::Starting.to_string()),
            Status::Unavailable(reason) => (RinzeStatus::Unavailable, reason),
        };
        service.message = c(&text);
        if !message.is_null() {
            unsafe { *message = service.message.as_ptr() };
        }
        status
    })
}

/// rinzed の入力状態 1 つ (1 接続) に当たる。入力欄ごとに作る。接続は最初の要求のときに張る。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_new(service: *const RinzeService) -> *mut RinzeClient {
    guarded(ptr::null_mut(), || {
        let Some(service) = (unsafe { service.as_ref() }) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(RinzeClient {
            client: Client::new(Arc::clone(&service.service)),
            failure: RinzeFailure::None,
            message: CString::default(),
            left_context: CString::default(),
        }))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_free(client: *mut RinzeClient) {
    guarded((), || {
        if !client.is_null() {
            drop(unsafe { Box::from_raw(client) });
        }
    })
}

fn finish(client: &mut RinzeClient, result: Result<Response, Error>) -> *mut RinzeResponse {
    let (failure, message) = match &result {
        Ok(_) => (RinzeFailure::None, String::new()),
        Err(e @ Error::Starting) => (RinzeFailure::Starting, e.to_string()),
        Err(e @ Error::Unavailable(_)) => (RinzeFailure::Unavailable, e.to_string()),
        Err(e @ Error::Delayed(_)) => (RinzeFailure::Delayed, e.to_string()),
        Err(e @ Error::Lost(_)) => (RinzeFailure::Lost, e.to_string()),
    };
    client.failure = failure;
    client.message = c(&message);
    match result {
        Ok(response) => Box::into_raw(Box::new(response.into())),
        Err(_) => ptr::null_mut(),
    }
}

fn request(
    client: *mut RinzeClient,
    f: impl FnOnce(&mut Client) -> Result<Response, Error>,
) -> *mut RinzeResponse {
    guarded(ptr::null_mut(), || match unsafe { client.as_mut() } {
        Some(client) => {
            let result = f(&mut client.client);
            finish(client, result)
        }
        None => ptr::null_mut(),
    })
}

/// `key` は rinzed のキーの名前 (1 文字か `space` `enter` `backspace` `ctrl-g` `ctrl-j` `ctrl-q`)。
/// `surrounding` は入力先のカーソルの前後の文字列、`cursor` はその中のカーソルの位置 (文字数)。取れなければ `surrounding` に NULL を渡す。
/// 失敗すると NULL を返し、理由は `rinze_client_failure` で読む。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_key(
    client: *mut RinzeClient,
    key: *const c_char,
    surrounding: *const c_char,
    cursor: usize,
) -> *mut RinzeResponse {
    let key = unsafe { text(key) }.unwrap_or_default();
    let surrounding = unsafe { text(surrounding) };
    let response = request(client, |c| {
        c.key(
            &key,
            surrounding
                .as_deref()
                .map(|text| Surrounding { text, cursor }),
        )
    });
    if let Some(client) = unsafe { client.as_mut() } {
        let text = client
            .client
            .last_left_context()
            .map_or("", |l: &LeftContext| &l.text);
        client.left_context = c(text);
    }
    response
}

/// 入力途中のものを確定せずに捨てる。入力モードは変えない。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_discard(client: *mut RinzeClient) -> *mut RinzeResponse {
    request(client, Client::discard)
}

/// 入力途中のものを表示どおりに確定する。入力モードは変えない。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_flush(client: *mut RinzeClient) -> *mut RinzeResponse {
    request(client, Client::flush)
}

/// 入力途中のものを確定してから、切り替えのキーを送って `mode` にする。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_switch_mode(
    client: *mut RinzeClient,
    mode: RinzeMode,
) -> *mut RinzeResponse {
    request(client, |c| c.switch_mode(mode.into()))
}

/// 直前の要求が失敗した理由。`message` には利用者に見せる文を入れる。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_failure(
    client: *const RinzeClient,
    message: *mut *const c_char,
) -> RinzeFailure {
    guarded(RinzeFailure::Lost, || {
        let Some(client) = (unsafe { client.as_ref() }) else {
            return RinzeFailure::Lost;
        };
        if !message.is_null() {
            unsafe { *message = client.message.as_ptr() };
        }
        client.failure
    })
}

/// 直前の `rinze_client_key` で送った左文脈と、その取り方。診断に使う。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_client_left_context(
    client: *const RinzeClient,
    text: *mut *const c_char,
) -> RinzeLeftContext {
    guarded(RinzeLeftContext::Unknown, || {
        let Some(client) = (unsafe { client.as_ref() }) else {
            return RinzeLeftContext::Unknown;
        };
        if !text.is_null() {
            unsafe { *text = client.left_context.as_ptr() };
        }
        match client.client.last_left_context() {
            None => RinzeLeftContext::Unknown,
            Some(LeftContext {
                origin: Origin::Surrounding,
                ..
            }) => RinzeLeftContext::Surrounding,
            Some(LeftContext {
                origin: Origin::PreeditRemoved,
                ..
            }) => RinzeLeftContext::PreeditRemoved,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_free(response: *mut RinzeResponse) {
    guarded((), || {
        if !response.is_null() {
            drop(unsafe { Box::from_raw(response) });
        }
    })
}

fn read<T: Copy>(
    response: *const RinzeResponse,
    fallback: T,
    f: impl FnOnce(&RinzeResponse) -> T,
) -> T {
    guarded(fallback, || match unsafe { response.as_ref() } {
        Some(r) => f(r),
        None => fallback,
    })
}

/// false なら、`commit` を確定したあと元のキーをアプリにそのまま渡す。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_handled(response: *const RinzeResponse) -> bool {
    read(response, false, |r| r.response.handled)
}

/// true の間 (ASCII モード) は `ctrl-j` 以外のキーを rinzed に送らずアプリに渡してよい。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_passthrough(response: *const RinzeResponse) -> bool {
    read(response, false, |r| r.response.passthrough)
}

/// 未確定文字列、候補一覧、入力欄のどれも無い。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_idle(response: *const RinzeResponse) -> bool {
    read(response, true, |r| r.response.is_idle())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_commit(response: *const RinzeResponse) -> *const c_char {
    read(response, EMPTY.as_ptr(), |r| r.commit.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_preedit(response: *const RinzeResponse) -> *const c_char {
    read(response, EMPTY.as_ptr(), |r| r.preedit.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_mode(response: *const RinzeResponse) -> RinzeMode {
    read(response, RinzeMode::Hiragana, |r| r.response.mode.into())
}

/// 候補一覧を表示していなければ 0。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_candidate_count(response: *const RinzeResponse) -> usize {
    read(response, 0, |r| r.candidates.len())
}

/// 候補を選ぶキー。`rinze_client_key` にそのまま渡すと、その候補を確定する。範囲の外なら空文字列。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_candidate_label(
    response: *const RinzeResponse,
    index: usize,
) -> *const c_char {
    read(response, EMPTY.as_ptr(), |r| {
        r.candidates
            .get(index)
            .map_or(EMPTY.as_ptr(), |(label, _)| label.as_ptr())
    })
}

/// 送りがなを含めた候補。範囲の外なら空文字列。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_candidate_word(
    response: *const RinzeResponse,
    index: usize,
) -> *const c_char {
    read(response, EMPTY.as_ptr(), |r| {
        r.candidates
            .get(index)
            .map_or(EMPTY.as_ptr(), |(_, word)| word.as_ptr())
    })
}

/// 表示中のページ (0 から)。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_page(response: *const RinzeResponse) -> usize {
    read(response, 0, |r| {
        r.response.candidates.as_ref().map_or(0, |c| c.page)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_pages(response: *const RinzeResponse) -> usize {
    read(response, 0, |r| {
        r.response.candidates.as_ref().map_or(0, |c| c.pages)
    })
}

/// 辞書登録の入力欄か、辞書から消すかの問いがあれば true を返し、`prompt` `text` `preedit` を入れる。この順に並べて表示する。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rinze_response_minibuffer(
    response: *const RinzeResponse,
    prompt: *mut *const c_char,
    text: *mut *const c_char,
    preedit: *mut *const c_char,
) -> bool {
    read(response, false, |r| {
        let Some([p, t, e]) = &r.minibuffer else {
            return false;
        };
        for (out, value) in [(prompt, p), (text, t), (preedit, e)] {
            if !out.is_null() {
                unsafe { *out = value.as_ptr() };
            }
        }
        true
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn header_matches_ffi() {
        let crate_dir = env!("CARGO_MANIFEST_DIR");
        let path = format!("{crate_dir}/include/rinze_client.h");
        let mut generated = Vec::new();
        cbindgen::generate(crate_dir)
            .expect("cbindgen でヘッダを作れる")
            .write(&mut generated);
        if std::env::var_os("RINZE_WRITE_HEADER").is_some() {
            std::fs::write(&path, &generated).unwrap();
        }
        let committed = std::fs::read(&path).unwrap();
        assert!(
            generated == committed,
            "include/rinze_client.h が ffi.rs とずれている。RINZE_WRITE_HEADER=1 cargo test -p rinze-client header で作り直す"
        );
    }
}
