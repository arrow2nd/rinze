//! SKK 方式の入力の状態機械。入出力を持たず、候補の取得は [`Converter`] に任せる。
//!
//! 1 つの [`Session`] が 1 つの入力先 (フロントエンドの 1 接続) の入力途中の状態を持つ。
//! フロントエンドは [`Session::handle`] にキーと左文脈を渡し、[`Response`] の確定文字列を挿入し、
//! 未確定文字列と候補一覧を表示する。
//!
//! 候補は辞書の候補、生成した表記の順に送り、それも送り切ると SKK と同じ辞書登録に入る。
//! 登録する語の入力欄も内側の [`Session`] で変換できるので、登録は入れ子になる。

mod romaji;

use std::mem;

use romaji::Romaji;

/// 候補をその場に 1 件ずつ表示する件数。これを超えると一覧に切り替える。
pub const INLINE_CANDIDATES: usize = 4;
/// 一覧の 1 ページの選択キー。
pub const PAGE_LABELS: [char; 7] = ['a', 's', 'd', 'f', 'j', 'k', 'l'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Space,
    Enter,
    Backspace,
    /// SKK の `C-g`。
    Cancel,
}

/// 送りがな。`consonant` は辞書の見出しに使う文字 (`ATte` なら `t`)、`kana` は確定した送りがな (`っ`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Okuri {
    pub consonant: char,
    pub kana: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Query<'a> {
    /// フロントエンドが送った左文脈に、同じキーの処理中に確定した文字列を足したもの。
    pub left_context: &'a str,
    /// 送りがなを含まない読み。
    pub reading: &'a str,
    pub okuri: Option<&'a Okuri>,
}

/// 候補はどれも送りがなを含まない。
pub trait Converter {
    /// 辞書の候補を表示する順に返す。
    fn candidates(&mut self, query: &Query) -> Vec<String>;
    /// 辞書に頼らずに表記を生成し、良い順に返す。辞書の候補を送り切ったとき (辞書に無ければ変換の開始時) に 1 度だけ呼ぶ。
    fn generate(&mut self, query: &Query) -> Vec<String>;
    /// 辞書登録で入力された語を保存する。
    fn register(&mut self, reading: &str, okuri: Option<&Okuri>, word: &str);
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Response {
    /// false ならフロントエンドは `commit` を挿入したあと、元のキーを通常どおり処理する。
    pub handled: bool,
    pub commit: String,
    pub preedit: String,
    /// 入力中の階層 (辞書登録中ならいちばん内側の入力欄) の候補一覧。
    pub candidates: Option<CandidatePage>,
    /// 辞書登録中だけ、いちばん内側の入力欄。
    pub registration: Option<RegistrationLine>,
}

/// 辞書登録の入力欄。フロントエンドは `prompt` `text` `preedit` をこの順に並べて表示する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationLine {
    /// `[辞書登録] まよねーず: `。入れ子の深さだけ括弧を重ねる。
    pub prompt: String,
    /// 入力欄に確定した文字列。
    pub text: String,
    /// 入力欄の未確定文字列。
    pub preedit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePage {
    /// 選択キーと、送りがなを含めた候補。
    pub items: Vec<(char, String)>,
    pub page: usize,
    pub pages: usize,
}

#[derive(Debug, Default)]
pub struct Session {
    roman: Romaji,
    phase: Phase,
}

#[derive(Debug, Default)]
enum Phase {
    #[default]
    Direct,
    Midashi { kana: String },
    Okuri { kana: String, consonant: char },
    Henkan(Conversion),
    Register(Box<Registration>),
}

#[derive(Debug)]
struct Conversion {
    reading: String,
    okuri: Option<Okuri>,
    candidates: Vec<String>,
    generated: bool,
    cursor: Cursor,
}

#[derive(Debug)]
struct Registration {
    conv: Conversion,
    text: String,
    inner: Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cursor {
    Inline(usize),
    Page(usize),
}

struct Step<'a> {
    left_context: &'a str,
    commit: String,
    converter: &'a mut dyn Converter,
}

impl Session {
    pub fn handle(
        &mut self,
        key: Key,
        left_context: &str,
        converter: &mut dyn Converter,
    ) -> Response {
        let mut step = Step {
            left_context,
            commit: String::new(),
            converter,
        };
        let handled = self.step(key, &mut step);
        Response {
            handled,
            commit: step.commit,
            preedit: self.preedit(),
            candidates: self.page(),
            registration: self.registration_line(1),
        }
    }

    /// 入力途中のものを表示どおりに確定し、状態を空にする。フロントエンドが入力先を離れるときに使う。
    pub fn flush(&mut self) -> Response {
        let commit = match mem::take(&mut self.phase) {
            Phase::Direct => self.roman.flush(),
            Phase::Midashi { kana } => kana + &self.roman.flush(),
            Phase::Okuri { kana, .. } => kana,
            Phase::Henkan(conv) => match conv.cursor {
                Cursor::Inline(i) => conv.surface(&conv.candidates[i]),
                Cursor::Page(_) => conv.surface(&conv.reading),
            },
            Phase::Register(reg) => reg.conv.surface(&reg.conv.reading),
        };
        self.roman.clear();
        Response {
            handled: true,
            commit,
            ..Response::default()
        }
    }

    fn step(&mut self, key: Key, step: &mut Step) -> bool {
        match mem::take(&mut self.phase) {
            Phase::Direct => self.direct(key, step),
            Phase::Midashi { kana } => self.midashi(kana, key, step),
            Phase::Okuri { kana, consonant } => self.okuri(kana, consonant, key, step),
            Phase::Henkan(conv) => self.henkan(conv, key, step),
            Phase::Register(reg) => self.register(reg, key, step),
        }
    }

    fn direct(&mut self, key: Key, step: &mut Step) -> bool {
        match key {
            Key::Char(c) if c.is_ascii_uppercase() => {
                step.commit.push_str(&self.roman.flush());
                self.midashi(String::new(), Key::Char(c.to_ascii_lowercase()), step)
            }
            Key::Char(c) => {
                step.commit.push_str(&self.roman.feed(c));
                true
            }
            Key::Space | Key::Enter => {
                step.commit.push_str(&self.roman.flush());
                false
            }
            Key::Backspace => self.roman.backspace(),
            Key::Cancel => {
                let had_pending = !self.roman.is_empty();
                self.roman.clear();
                had_pending
            }
        }
    }

    fn midashi(&mut self, mut kana: String, key: Key, step: &mut Step) -> bool {
        match key {
            Key::Char(c) if c.is_ascii_uppercase() && !kana.is_empty() => {
                kana.push_str(&self.roman.flush());
                let consonant = c.to_ascii_lowercase();
                return self.okuri(kana, consonant, Key::Char(consonant), step);
            }
            Key::Char(c) => kana.push_str(&self.roman.feed(c.to_ascii_lowercase())),
            Key::Space => {
                kana.push_str(&self.roman.flush());
                if !kana.is_empty() {
                    self.convert(kana, None, step);
                    return true;
                }
            }
            Key::Enter => {
                kana.push_str(&self.roman.flush());
                step.commit.push_str(&kana);
                return true;
            }
            Key::Backspace => {
                if !self.roman.backspace() && kana.pop().is_none() {
                    return true;
                }
            }
            Key::Cancel => {
                self.roman.clear();
                return true;
            }
        }
        self.phase = Phase::Midashi { kana };
        true
    }

    fn okuri(&mut self, kana: String, consonant: char, key: Key, step: &mut Step) -> bool {
        match key {
            Key::Char(c) => {
                let okuri = self.roman.feed(c.to_ascii_lowercase());
                if okuri.is_empty() {
                    self.phase = Phase::Okuri { kana, consonant };
                } else {
                    self.convert(
                        kana,
                        Some(Okuri {
                            consonant,
                            kana: okuri,
                        }),
                        step,
                    );
                }
            }
            Key::Space => self.phase = Phase::Okuri { kana, consonant },
            Key::Enter => {
                self.roman.clear();
                step.commit.push_str(&kana);
            }
            Key::Backspace => {
                self.roman.clear();
                self.phase = Phase::Midashi { kana };
            }
            Key::Cancel => self.roman.clear(),
        }
        true
    }

    fn convert(&mut self, reading: String, okuri: Option<Okuri>, step: &mut Step) {
        let mut conv = Conversion {
            reading,
            okuri,
            candidates: Vec::new(),
            generated: false,
            cursor: Cursor::Inline(0),
        };
        let left_context = step.left_context();
        conv.candidates = step.converter.candidates(&conv.query(&left_context));
        if conv.candidates.is_empty() {
            conv.generate(step);
        }
        self.phase = if conv.candidates.is_empty() {
            Phase::Register(Box::new(Registration::new(conv)))
        } else {
            Phase::Henkan(conv)
        };
    }

    fn henkan(&mut self, mut conv: Conversion, key: Key, step: &mut Step) -> bool {
        match (key, conv.cursor) {
            (Key::Space, _) => {
                if !conv.next() {
                    let shown = conv.shown();
                    if conv.generated || !conv.generate(step) {
                        self.phase = Phase::Register(Box::new(Registration::new(conv)));
                        return true;
                    }
                    if conv.shown() == shown {
                        conv.next();
                    }
                }
            }
            (Key::Char('x'), _) | (Key::Backspace, Cursor::Page(_)) => {
                if !conv.prev() {
                    self.roman.clear();
                    self.phase = Phase::Midashi { kana: conv.reading };
                    return true;
                }
            }
            (Key::Cancel, _) => {
                self.roman.clear();
                self.phase = Phase::Midashi { kana: conv.reading };
                return true;
            }
            (Key::Char(c), Cursor::Page(page)) => {
                if let Some(word) = conv.page_item(page, c) {
                    step.commit.push_str(&word);
                    return true;
                }
            }
            (_, Cursor::Page(_)) => {}
            (Key::Enter, Cursor::Inline(i)) => {
                step.commit.push_str(&conv.surface(&conv.candidates[i]));
                return true;
            }
            (Key::Backspace, Cursor::Inline(i)) => {
                let mut word = conv.surface(&conv.candidates[i]);
                word.pop();
                step.commit.push_str(&word);
                return true;
            }
            (Key::Char(_), Cursor::Inline(i)) => {
                step.commit.push_str(&conv.surface(&conv.candidates[i]));
                return self.direct(key, step);
            }
        }
        self.phase = Phase::Henkan(conv);
        true
    }

    fn register(&mut self, mut reg: Box<Registration>, key: Key, step: &mut Step) -> bool {
        let left_context = format!("{}{}", step.left_context(), reg.text);
        let res = reg.inner.handle(key, &left_context, &mut *step.converter);
        reg.text.push_str(&res.commit);
        if !res.handled {
            match key {
                Key::Enter if !reg.text.is_empty() => {
                    let word = reg.word();
                    step.converter
                        .register(&reg.conv.reading, reg.conv.okuri.as_ref(), word);
                    step.commit.push_str(&reg.conv.surface(word));
                    return true;
                }
                Key::Enter | Key::Cancel => {
                    self.phase = reg.cancel();
                    return true;
                }
                Key::Backspace => {
                    reg.text.pop();
                }
                Key::Space => reg.text.push(' '),
                Key::Char(c) => reg.text.push(c),
            }
        }
        self.phase = Phase::Register(reg);
        true
    }

    fn preedit(&self) -> String {
        let roman = self.roman.pending();
        match &self.phase {
            Phase::Direct => roman.to_string(),
            Phase::Midashi { kana } => format!("▽{kana}{roman}"),
            Phase::Okuri { kana, .. } => format!("▽{kana}*{roman}"),
            Phase::Henkan(conv) => match conv.cursor {
                Cursor::Inline(i) => format!("▼{}{roman}", conv.surface(&conv.candidates[i])),
                Cursor::Page(_) => format!("▼{}{roman}", conv.surface(&conv.reading)),
            },
            Phase::Register(reg) => format!("▼{}", reg.conv.surface(&reg.conv.reading)),
        }
    }

    fn registration_line(&self, depth: usize) -> Option<RegistrationLine> {
        let Phase::Register(reg) = &self.phase else {
            return None;
        };
        reg.inner.registration_line(depth + 1).or_else(|| {
            let midashi = match &reg.conv.okuri {
                Some(okuri) => format!("{}*{}", reg.conv.reading, okuri.kana),
                None => reg.conv.reading.clone(),
            };
            Some(RegistrationLine {
                prompt: format!(
                    "{}辞書登録{} {midashi}: ",
                    "[".repeat(depth),
                    "]".repeat(depth)
                ),
                text: reg.text.clone(),
                preedit: reg.inner.preedit(),
            })
        })
    }

    fn page(&self) -> Option<CandidatePage> {
        let conv = match &self.phase {
            Phase::Henkan(conv) => conv,
            Phase::Register(reg) => return reg.inner.page(),
            _ => return None,
        };
        let Cursor::Page(page) = conv.cursor else {
            return None;
        };
        let items = conv
            .page_words(page)
            .iter()
            .zip(PAGE_LABELS)
            .map(|(word, label)| (label, conv.surface(word)))
            .collect();
        Some(CandidatePage {
            items,
            page,
            pages: conv.pages(),
        })
    }
}

impl Step<'_> {
    fn left_context(&self) -> String {
        format!("{}{}", self.left_context, self.commit)
    }
}

impl Registration {
    fn new(conv: Conversion) -> Self {
        Self {
            conv,
            text: String::new(),
            inner: Session::default(),
        }
    }

    fn word(&self) -> &str {
        self.conv
            .okuri
            .as_ref()
            .and_then(|o| self.text.strip_suffix(o.kana.as_str()))
            .filter(|w| !w.is_empty())
            .unwrap_or(&self.text)
    }

    fn cancel(self) -> Phase {
        if self.conv.candidates.is_empty() {
            Phase::Midashi {
                kana: self.conv.surface(&self.conv.reading),
            }
        } else {
            Phase::Henkan(self.conv)
        }
    }
}

impl Conversion {
    fn query<'a>(&'a self, left_context: &'a str) -> Query<'a> {
        Query {
            left_context,
            reading: &self.reading,
            okuri: self.okuri.as_ref(),
        }
    }

    fn generate(&mut self, step: &mut Step) -> bool {
        self.generated = true;
        let left_context = step.left_context();
        let words = step.converter.generate(&self.query(&left_context));
        let before = self.candidates.len();
        for word in words {
            if !self.candidates.contains(&word) {
                self.candidates.push(word);
            }
        }
        self.candidates.len() > before
    }

    fn shown(&self) -> usize {
        match self.cursor {
            Cursor::Inline(_) => 1,
            Cursor::Page(page) => self.page_words(page).len(),
        }
    }

    fn surface(&self, word: &str) -> String {
        format!("{word}{}", self.okuri.as_ref().map_or("", |o| &o.kana))
    }

    fn pages(&self) -> usize {
        self.candidates
            .len()
            .saturating_sub(INLINE_CANDIDATES)
            .div_ceil(PAGE_LABELS.len())
    }

    fn page_words(&self, page: usize) -> &[String] {
        let start = INLINE_CANDIDATES + page * PAGE_LABELS.len();
        let end = (start + PAGE_LABELS.len()).min(self.candidates.len());
        &self.candidates[start..end]
    }

    fn page_item(&self, page: usize, label: char) -> Option<String> {
        let index = PAGE_LABELS.iter().position(|&l| l == label)?;
        self.page_words(page).get(index).map(|w| self.surface(w))
    }

    fn next(&mut self) -> bool {
        let n = self.candidates.len();
        self.cursor = match self.cursor {
            Cursor::Inline(i) if i + 1 < n.min(INLINE_CANDIDATES) => Cursor::Inline(i + 1),
            Cursor::Inline(i) if i + 1 == INLINE_CANDIDATES && n > INLINE_CANDIDATES => {
                Cursor::Page(0)
            }
            Cursor::Page(p) if p + 1 < self.pages() => Cursor::Page(p + 1),
            _ => return false,
        };
        true
    }

    fn prev(&mut self) -> bool {
        self.cursor = match self.cursor {
            Cursor::Inline(0) => return false,
            Cursor::Inline(i) => Cursor::Inline(i - 1),
            Cursor::Page(0) => Cursor::Inline(INLINE_CANDIDATES - 1),
            Cursor::Page(p) => Cursor::Page(p - 1),
        };
        true
    }
}
