//! SKK 方式の入力の状態機械。入出力を持たず、候補の取得は [`Converter`] に任せる。
//!
//! 1 つの [`Session`] が 1 つの入力先 (フロントエンドの 1 接続) の入力モードと入力途中の状態を持つ。
//! フロントエンドは [`Session::handle`] にキーと左文脈を渡し、[`Response`] の確定文字列を挿入し、
//! 未確定文字列と候補一覧、入力モードを表示する。
//!
//! 入力モードは SKK と同じく、かな系 (ひらがな、カタカナ、半角カナ) と英字系 (ASCII、全角英字) に分かれる。
//! 見出し (▽)、変換 (▼)、辞書登録はかな系のモードにしか無い。読みはどのモードでもひらがなで持ち、
//! 表示と確定のときにモードの文字種にする。
//!
//! 候補は辞書の候補、生成した表記の順に送り、それも送り切ると SKK と同じ辞書登録に入る。
//! 登録する語の入力欄も内側の [`Session`] で変換できるので、登録は入れ子になる。
//! 入力欄は外側とは別のモードを持ち、ddskk と同じくひらがなで始まる。

mod romaji;
mod script;

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
    /// SKK の `C-j`。
    Kakutei,
    /// SKK の `C-q`。
    ToggleHalfwidth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Hiragana,
    Katakana,
    HalfwidthKatakana,
    /// キーをそのまま通す直接入力。
    Ascii,
    FullwidthLatin,
}

/// 送りがな。`consonant` は辞書の見出しに使う文字 (`ATte` なら `t`)、`kana` は確定した送りがな (`っ`)。
/// `kana` はモードによらずひらがな。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Okuri {
    pub consonant: char,
    pub kana: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Query<'a> {
    /// フロントエンドが送った左文脈に、同じキーの処理中に確定した文字列を足したもの。
    pub left_context: &'a str,
    /// 送りがなを含まない読み。モードによらずひらがな。
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
    /// 次のキーを受ける階層 (辞書登録中ならいちばん内側の入力欄) のモード。
    pub mode: Mode,
    /// true の間は [`Key::Kakutei`] 以外のキーをすべて処理せずに返す (`handled` は false、`commit` は空)。
    /// フロントエンドは [`Key::Kakutei`] 以外のキーを送らずに、そのまま処理してよい。
    pub passthrough: bool,
}

/// 辞書登録の入力欄。フロントエンドは `prompt` `text` `preedit` をこの順に並べて表示する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationLine {
    /// `[辞書登録] まよねーず: `。入れ子の深さだけ括弧を重ねる。見出しはモードによらずひらがな。
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
    input: Input,
}

#[derive(Debug)]
enum Input {
    Kana(KanaInput),
    Latin(Latin),
}

#[derive(Debug, Default)]
struct KanaInput {
    script: Script,
    phase: Phase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Script {
    #[default]
    Hiragana,
    Katakana,
    HalfwidthKatakana,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Latin {
    Ascii,
    Fullwidth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Switch {
    ToggleKatakana,
    ToggleHalfwidth,
    Ascii,
    FullwidthLatin,
    Kakutei,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KanaKey {
    Switch(Switch),
    Char(char),
    Space,
    Enter,
    Backspace,
    Cancel,
}

#[derive(Debug)]
enum Phase {
    Direct {
        roman: Romaji,
    },
    Midashi {
        kana: String,
        roman: Romaji,
    },
    Okuri {
        kana: String,
        consonant: char,
        roman: Romaji,
    },
    Henkan {
        conv: Conversion,
        trailing_roman: Romaji,
    },
    Register {
        reg: Box<Registration>,
        trailing_roman: Romaji,
    },
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Direct {
            roman: Romaji::default(),
        }
    }
}

#[derive(Debug)]
struct Reading {
    kana: String,
    okuri: Option<Okuri>,
}

#[derive(Debug)]
struct Conversion {
    reading: Reading,
    candidates: Candidates,
    generated: bool,
    cursor: Cursor,
}

#[derive(Debug)]
struct Candidates(Vec<String>);

#[derive(Debug)]
struct Registration {
    origin: Origin,
    text: String,
    inner: Session,
}

#[derive(Debug)]
enum Origin {
    Candidates(Conversion),
    NoCandidates(Reading),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cursor {
    Inline(usize),
    Page(usize),
}

struct Step<'a> {
    left_context: &'a str,
    commit: String,
    next_mode: Option<Mode>,
    converter: &'a mut dyn Converter,
}

impl Default for Input {
    fn default() -> Self {
        Input::Kana(KanaInput::default())
    }
}

impl Input {
    fn new(mode: Mode) -> Self {
        let kana = |script| {
            Input::Kana(KanaInput {
                script,
                phase: Phase::default(),
            })
        };
        match mode {
            Mode::Hiragana => kana(Script::Hiragana),
            Mode::Katakana => kana(Script::Katakana),
            Mode::HalfwidthKatakana => kana(Script::HalfwidthKatakana),
            Mode::Ascii => Input::Latin(Latin::Ascii),
            Mode::FullwidthLatin => Input::Latin(Latin::Fullwidth),
        }
    }
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
            next_mode: None,
            converter,
        };
        let handled = match &mut self.input {
            Input::Kana(kana) => kana.step(key, &mut step),
            Input::Latin(latin) => latin.handle(key, &mut step),
        };
        if let Some(mode) = step.next_mode {
            self.input = Input::new(mode);
        }
        self.response(handled, step.commit)
    }

    /// 入力途中のものを表示どおりに確定し、状態を空にする。モードは変えない。フロントエンドが入力先を離れるときに使う。
    pub fn flush(&mut self) -> Response {
        let commit = self.take_pending();
        self.response(true, commit)
    }

    /// [`Session::flush`] したうえで、モードを始めのひらがなに戻す。フロントエンドが入力を有効にしたときに使う。
    pub fn reset(&mut self) -> Response {
        let commit = self.take_pending();
        self.input = Input::default();
        self.response(true, commit)
    }

    fn take_pending(&mut self) -> String {
        match &mut self.input {
            Input::Kana(kana) => kana.flush(),
            Input::Latin(_) => String::new(),
        }
    }

    fn response(&self, handled: bool, commit: String) -> Response {
        Response {
            handled,
            commit,
            preedit: self.preedit(),
            candidates: self.page(),
            registration: self.registration_line(1),
            mode: self.mode(),
            passthrough: matches!(self.input, Input::Latin(Latin::Ascii)),
        }
    }

    fn mode(&self) -> Mode {
        match &self.input {
            Input::Kana(KanaInput {
                phase: Phase::Register { reg, .. },
                ..
            }) => reg.inner.mode(),
            Input::Kana(kana) => kana.script.into(),
            Input::Latin(latin) => (*latin).into(),
        }
    }

    fn preedit(&self) -> String {
        match &self.input {
            Input::Kana(kana) => kana.preedit(),
            Input::Latin(_) => String::new(),
        }
    }

    fn registration_line(&self, depth: usize) -> Option<RegistrationLine> {
        let Input::Kana(kana) = &self.input else {
            return None;
        };
        kana.registration_line(depth)
    }

    fn page(&self) -> Option<CandidatePage> {
        let Input::Kana(kana) = &self.input else {
            return None;
        };
        kana.page()
    }
}

impl From<Script> for Mode {
    fn from(script: Script) -> Self {
        match script {
            Script::Hiragana => Mode::Hiragana,
            Script::Katakana => Mode::Katakana,
            Script::HalfwidthKatakana => Mode::HalfwidthKatakana,
        }
    }
}

impl From<Latin> for Mode {
    fn from(latin: Latin) -> Self {
        match latin {
            Latin::Ascii => Mode::Ascii,
            Latin::Fullwidth => Mode::FullwidthLatin,
        }
    }
}

impl Latin {
    fn handle(self, key: Key, step: &mut Step) -> bool {
        match (self, key) {
            (_, Key::Kakutei) => step.next_mode = Some(Mode::Hiragana),
            // ddskk の全英モードの `C-q` は `skk-toggle-characters` で、カナモードに入る
            (Latin::Fullwidth, Key::ToggleHalfwidth) => step.next_mode = Some(Mode::Katakana),
            (Latin::Fullwidth, Key::Char(c)) => match script::fullwidth_latin(c) {
                Some(wide) => step.commit.push(wide),
                None => return false,
            },
            (Latin::Fullwidth, Key::Space) => step.commit.push('　'),
            _ => return false,
        }
        true
    }
}

impl KanaKey {
    fn of(key: Key, roman: &Romaji) -> Self {
        match key {
            Key::Char(c) if roman.continues(c) => KanaKey::Char(c),
            Key::Char('q') => KanaKey::Switch(Switch::ToggleKatakana),
            Key::Char('l') => KanaKey::Switch(Switch::Ascii),
            Key::Char('L') => KanaKey::Switch(Switch::FullwidthLatin),
            Key::Char(c) => KanaKey::Char(c),
            Key::Space => KanaKey::Space,
            Key::Enter => KanaKey::Enter,
            Key::Backspace => KanaKey::Backspace,
            Key::Cancel => KanaKey::Cancel,
            Key::Kakutei => KanaKey::Switch(Switch::Kakutei),
            Key::ToggleHalfwidth => KanaKey::Switch(Switch::ToggleHalfwidth),
        }
    }
}

impl Script {
    fn render(self, hiragana: &str) -> String {
        match self {
            Script::Hiragana => hiragana.to_string(),
            Script::Katakana => script::katakana(hiragana),
            Script::HalfwidthKatakana => script::halfwidth_katakana(hiragana),
        }
    }

    fn after(self, switch: Switch) -> Mode {
        match (switch, self) {
            (Switch::ToggleKatakana, Script::Hiragana) => Mode::Katakana,
            (Switch::ToggleKatakana | Switch::ToggleHalfwidth, Script::HalfwidthKatakana) => {
                Mode::Hiragana
            }
            (Switch::ToggleKatakana, Script::Katakana) => Mode::Hiragana,
            (Switch::ToggleHalfwidth, _) => Mode::HalfwidthKatakana,
            (Switch::Ascii, _) => Mode::Ascii,
            (Switch::FullwidthLatin, _) => Mode::FullwidthLatin,
            (Switch::Kakutei, _) => self.into(),
        }
    }

    fn midashi_converted(self, switch: Switch) -> Option<Script> {
        match (switch, self) {
            (Switch::ToggleKatakana, Script::Hiragana) => Some(Script::Katakana),
            (Switch::ToggleKatakana, Script::Katakana) => Some(Script::Hiragana),
            (Switch::ToggleKatakana | Switch::ToggleHalfwidth, _) => {
                Some(Script::HalfwidthKatakana)
            }
            (Switch::Ascii | Switch::FullwidthLatin | Switch::Kakutei, _) => None,
        }
    }
}

impl KanaInput {
    fn step(&mut self, key: Key, step: &mut Step) -> bool {
        match mem::take(&mut self.phase) {
            Phase::Direct { roman } => self.direct(roman, key, step),
            Phase::Midashi { kana, roman } => self.midashi(kana, roman, key, step),
            Phase::Okuri {
                kana,
                consonant,
                roman,
            } => self.okuri(kana, consonant, roman, key, step),
            Phase::Henkan {
                conv,
                trailing_roman,
            } => self.henkan(conv, trailing_roman, key, step),
            Phase::Register {
                reg,
                trailing_roman,
            } => self.register(reg, trailing_roman, key, step),
        }
    }

    fn flush(&mut self) -> String {
        let hiragana = match mem::take(&mut self.phase) {
            Phase::Direct { mut roman } => roman.flush_n_discard_rest().to_string(),
            Phase::Midashi { kana, mut roman } => kana + roman.flush_n_discard_rest(),
            Phase::Okuri { kana, .. } => kana,
            Phase::Henkan { conv, .. } => return conv.displayed(self.script),
            Phase::Register { reg, .. } => reg.reading().kana_surface(),
        };
        self.script.render(&hiragana)
    }

    fn direct(&mut self, mut roman: Romaji, key: Key, step: &mut Step) -> bool {
        let handled = match KanaKey::of(key, &roman) {
            KanaKey::Switch(switch) => {
                step.commit
                    .push_str(&self.script.render(roman.flush_n_discard_rest()));
                step.next_mode = Some(self.script.after(switch));
                return true;
            }
            KanaKey::Char(c) if c.is_ascii_uppercase() => {
                step.commit
                    .push_str(&self.script.render(roman.flush_n_discard_rest()));
                let mut roman = Romaji::default();
                let kana = roman.feed(c.to_ascii_lowercase());
                self.phase = Phase::Midashi { kana, roman };
                return true;
            }
            KanaKey::Char(c) => {
                step.commit.push_str(&self.script.render(&roman.feed(c)));
                true
            }
            KanaKey::Space | KanaKey::Enter => {
                step.commit
                    .push_str(&self.script.render(roman.flush_n_discard_rest()));
                false
            }
            KanaKey::Backspace => roman.backspace(),
            KanaKey::Cancel => {
                let had_pending = !roman.is_empty();
                roman.clear();
                had_pending
            }
        };
        self.phase = Phase::Direct { roman };
        handled
    }

    fn commit_midashi(&self, kana: &str, switch: Switch, step: &mut Step) {
        match self.script.midashi_converted(switch) {
            Some(script) => step.commit.push_str(&script.render(kana)),
            None => {
                step.commit.push_str(&self.script.render(kana));
                step.next_mode = Some(self.script.after(switch));
            }
        }
    }

    fn midashi(&mut self, mut kana: String, mut roman: Romaji, key: Key, step: &mut Step) -> bool {
        match KanaKey::of(key, &roman) {
            KanaKey::Switch(switch) => {
                kana.push_str(roman.flush_n_discard_rest());
                self.commit_midashi(&kana, switch, step);
                return true;
            }
            KanaKey::Char(c) if c.is_ascii_uppercase() && !kana.is_empty() => {
                kana.push_str(roman.flush_n_discard_rest());
                let consonant = c.to_ascii_lowercase();
                self.feed_okuri(kana, consonant, Romaji::default(), consonant, step);
                return true;
            }
            KanaKey::Char(c) => kana.push_str(&roman.feed(c.to_ascii_lowercase())),
            KanaKey::Space => {
                kana.push_str(roman.flush_n_discard_rest());
                if !kana.is_empty() {
                    let reading = Reading { kana, okuri: None };
                    self.convert(reading, Romaji::default(), step);
                    return true;
                }
            }
            KanaKey::Enter => {
                kana.push_str(roman.flush_n_discard_rest());
                step.commit.push_str(&self.script.render(&kana));
                return true;
            }
            KanaKey::Backspace => {
                if !roman.backspace() && kana.pop().is_none() {
                    return true;
                }
            }
            KanaKey::Cancel => return true,
        }
        self.phase = Phase::Midashi { kana, roman };
        true
    }

    fn okuri(
        &mut self,
        kana: String,
        consonant: char,
        roman: Romaji,
        key: Key,
        step: &mut Step,
    ) -> bool {
        match KanaKey::of(key, &roman) {
            KanaKey::Switch(switch) => self.commit_midashi(&kana, switch, step),
            KanaKey::Char(c) => self.feed_okuri(kana, consonant, roman, c, step),
            KanaKey::Space => {
                self.phase = Phase::Okuri {
                    kana,
                    consonant,
                    roman,
                }
            }
            KanaKey::Enter => step.commit.push_str(&self.script.render(&kana)),
            KanaKey::Backspace => {
                self.phase = Phase::Midashi {
                    kana,
                    roman: Romaji::default(),
                }
            }
            KanaKey::Cancel => {}
        }
        true
    }

    fn feed_okuri(
        &mut self,
        kana: String,
        consonant: char,
        mut roman: Romaji,
        c: char,
        step: &mut Step,
    ) {
        let okuri = roman.feed(c.to_ascii_lowercase());
        if okuri.is_empty() {
            self.phase = Phase::Okuri {
                kana,
                consonant,
                roman,
            };
        } else {
            let reading = Reading {
                kana,
                okuri: Some(Okuri {
                    consonant,
                    kana: okuri,
                }),
            };
            self.convert(reading, roman, step);
        }
    }

    fn convert(&mut self, reading: Reading, trailing_roman: Romaji, step: &mut Step) {
        let left_context = step.left_context_with_commit();
        let query = reading.query(&left_context);
        let mut words = step.converter.candidates(&query);
        let generated = words.is_empty();
        if generated {
            push_unique(&mut words, step.converter.generate(&query));
        }
        self.phase = match Candidates::new(words) {
            Some(candidates) => Phase::Henkan {
                conv: Conversion {
                    reading,
                    candidates,
                    generated,
                    cursor: Cursor::Inline(0),
                },
                trailing_roman,
            },
            None => Phase::Register {
                reg: Box::new(Registration::new(Origin::NoCandidates(reading))),
                trailing_roman,
            },
        };
    }

    fn henkan(
        &mut self,
        mut conv: Conversion,
        trailing_roman: Romaji,
        key: Key,
        step: &mut Step,
    ) -> bool {
        match (key, conv.cursor) {
            (Key::Space, _) => {
                if !conv.next() && !conv.advance_to_generated(step) {
                    self.phase = Phase::Register {
                        reg: Box::new(Registration::new(Origin::Candidates(conv))),
                        trailing_roman,
                    };
                    return true;
                }
            }
            (Key::Char('x'), _) | (Key::Backspace, Cursor::Page(_)) => {
                if !conv.prev() {
                    self.phase = Phase::Midashi {
                        kana: conv.reading.kana,
                        roman: Romaji::default(),
                    };
                    return true;
                }
            }
            (Key::Cancel, _) => {
                self.phase = Phase::Midashi {
                    kana: conv.reading.kana,
                    roman: Romaji::default(),
                };
                return true;
            }
            (Key::Char(c), Cursor::Page(page)) => {
                if let Some(word) = conv.page_item(page, c, self.script) {
                    step.commit.push_str(&word);
                    self.phase = Phase::Direct {
                        roman: trailing_roman,
                    };
                    return true;
                }
            }
            (_, Cursor::Page(_)) => {}
            (Key::Enter, Cursor::Inline(_)) => {
                step.commit.push_str(&conv.displayed(self.script));
                self.phase = Phase::Direct {
                    roman: trailing_roman,
                };
                return true;
            }
            (Key::Backspace, Cursor::Inline(_)) => {
                let mut word = conv.displayed(self.script);
                word.pop();
                step.commit.push_str(&word);
                self.phase = Phase::Direct {
                    roman: trailing_roman,
                };
                return true;
            }
            (Key::Char(_) | Key::Kakutei | Key::ToggleHalfwidth, Cursor::Inline(_)) => {
                step.commit.push_str(&conv.displayed(self.script));
                return self.direct(trailing_roman, key, step);
            }
        }
        self.phase = Phase::Henkan {
            conv,
            trailing_roman,
        };
        true
    }

    fn register(
        &mut self,
        mut reg: Box<Registration>,
        trailing_roman: Romaji,
        key: Key,
        step: &mut Step,
    ) -> bool {
        let left_context = reg.field_left_context(&step.left_context_with_commit());
        let res = reg.inner.handle(key, &left_context, &mut *step.converter);
        reg.text.push_str(&res.commit);
        if !res.handled {
            match key {
                Key::Enter if !reg.text.is_empty() => {
                    let word = reg.word_without_okuri();
                    let reading = reg.reading();
                    step.converter
                        .register(&reading.kana, reading.okuri.as_ref(), word);
                    step.commit.push_str(&reading.surface(word, self.script));
                    self.phase = Phase::Direct {
                        roman: trailing_roman,
                    };
                    return true;
                }
                Key::Enter | Key::Cancel => {
                    self.phase = reg.cancel(trailing_roman);
                    return true;
                }
                Key::Backspace => {
                    reg.text.pop();
                }
                Key::Space => reg.text.push(' '),
                Key::Char(c) => reg.text.push(c),
                Key::Kakutei | Key::ToggleHalfwidth => {}
            }
        }
        self.phase = Phase::Register {
            reg,
            trailing_roman,
        };
        true
    }

    fn preedit(&self) -> String {
        let render = |hiragana: &str| self.script.render(hiragana);
        match &self.phase {
            Phase::Direct { roman } => roman.pending().to_string(),
            Phase::Midashi { kana, roman } => format!("▽{}{}", render(kana), roman.pending()),
            Phase::Okuri { kana, roman, .. } => format!("▽{}*{}", render(kana), roman.pending()),
            Phase::Henkan {
                conv,
                trailing_roman,
            } => format!(
                "▼{}{}",
                conv.displayed(self.script),
                trailing_roman.pending()
            ),
            Phase::Register { reg, .. } => format!("▼{}", render(&reg.reading().kana_surface())),
        }
    }

    fn registration_line(&self, depth: usize) -> Option<RegistrationLine> {
        let Phase::Register { reg, .. } = &self.phase else {
            return None;
        };
        reg.inner.registration_line(depth + 1).or_else(|| {
            Some(RegistrationLine {
                prompt: format!(
                    "{}辞書登録{} {}: ",
                    "[".repeat(depth),
                    "]".repeat(depth),
                    reg.reading().midashi()
                ),
                text: reg.text.clone(),
                preedit: reg.inner.preedit(),
            })
        })
    }

    fn page(&self) -> Option<CandidatePage> {
        let conv = match &self.phase {
            Phase::Henkan { conv, .. } => conv,
            Phase::Register { reg, .. } => return reg.inner.page(),
            _ => return None,
        };
        let Cursor::Page(page) = conv.cursor else {
            return None;
        };
        let items = conv
            .page_words(page)
            .iter()
            .zip(PAGE_LABELS)
            .map(|(word, label)| (label, conv.reading.surface(word, self.script)))
            .collect();
        Some(CandidatePage {
            items,
            page,
            pages: conv.pages(),
        })
    }
}

impl Step<'_> {
    fn left_context_with_commit(&self) -> String {
        format!("{}{}", self.left_context, self.commit)
    }
}

impl Reading {
    fn query<'a>(&'a self, left_context: &'a str) -> Query<'a> {
        Query {
            left_context,
            reading: &self.kana,
            okuri: self.okuri.as_ref(),
        }
    }

    fn surface(&self, word: &str, script: Script) -> String {
        let okuri = self
            .okuri
            .as_ref()
            .map_or(String::new(), |o| script.render(&o.kana));
        format!("{word}{okuri}")
    }

    fn kana_surface(&self) -> String {
        format!(
            "{}{}",
            self.kana,
            self.okuri.as_ref().map_or("", |o| &o.kana)
        )
    }

    fn midashi(&self) -> String {
        match &self.okuri {
            Some(okuri) => format!("{}*{}", self.kana, okuri.kana),
            None => self.kana.clone(),
        }
    }
}

impl Registration {
    fn new(origin: Origin) -> Self {
        Self {
            origin,
            text: String::new(),
            inner: Session::default(),
        }
    }

    fn reading(&self) -> &Reading {
        match &self.origin {
            Origin::Candidates(conv) => &conv.reading,
            Origin::NoCandidates(reading) => reading,
        }
    }

    fn field_left_context(&self, outer_left_context: &str) -> String {
        format!("{outer_left_context}{}", self.text)
    }

    fn word_without_okuri(&self) -> &str {
        self.reading()
            .okuri
            .as_ref()
            .and_then(|o| self.text.strip_suffix(o.kana.as_str()))
            .filter(|w| !w.is_empty())
            .unwrap_or(&self.text)
    }

    fn cancel(self, trailing_roman: Romaji) -> Phase {
        match self.origin {
            Origin::Candidates(conv) => Phase::Henkan {
                conv,
                trailing_roman,
            },
            Origin::NoCandidates(reading) => Phase::Midashi {
                kana: reading.kana_surface(),
                roman: trailing_roman,
            },
        }
    }
}

impl Candidates {
    fn new(words: Vec<String>) -> Option<Self> {
        (!words.is_empty()).then_some(Self(words))
    }

    fn extend_unique(&mut self, words: Vec<String>) -> bool {
        push_unique(&mut self.0, words)
    }
}

impl std::ops::Deref for Candidates {
    type Target = [String];

    fn deref(&self) -> &[String] {
        &self.0
    }
}

fn push_unique(list: &mut Vec<String>, words: Vec<String>) -> bool {
    let before = list.len();
    for word in words {
        if !list.contains(&word) {
            list.push(word);
        }
    }
    list.len() > before
}

impl Conversion {
    fn displayed(&self, script: Script) -> String {
        match self.cursor {
            Cursor::Inline(i) => self.reading.surface(&self.candidates[i], script),
            Cursor::Page(_) => script.render(&self.reading.kana_surface()),
        }
    }

    fn advance_to_generated(&mut self, step: &mut Step) -> bool {
        if self.generated {
            return false;
        }
        self.generated = true;
        let shown_before = self.shown();
        let left_context = step.left_context_with_commit();
        let words = step.converter.generate(&self.reading.query(&left_context));
        if !self.candidates.extend_unique(words) {
            return false;
        }
        let added_to_current_page = self.shown() > shown_before;
        if !added_to_current_page {
            self.next();
        }
        true
    }

    fn shown(&self) -> usize {
        match self.cursor {
            Cursor::Inline(_) => 1,
            Cursor::Page(page) => self.page_words(page).len(),
        }
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

    fn page_item(&self, page: usize, label: char, script: Script) -> Option<String> {
        let index = PAGE_LABELS.iter().position(|&l| l == label)?;
        self.page_words(page)
            .get(index)
            .map(|w| self.reading.surface(w, script))
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
