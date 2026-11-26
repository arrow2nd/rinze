//! SKK 方式の入力の状態機械。入出力を持たず、候補の取得は [`Converter`] に任せる。
//!
//! 1 つの [`Session`] が 1 つの入力先 (フロントエンドの 1 接続) の入力モードと入力途中の状態を持つ。
//! フロントエンドは [`Session::handle`] にキーと左文脈を渡し、[`Response`] の確定文字列を挿入し、
//! 未確定文字列と候補一覧、入力モードを表示する。
//!
//! 入力モードは SKK と同じく、かな系 (ひらがな、カタカナ、半角カナ) と英字系 (ASCII、全角英字) に分かれる。
//! 見出し (▽)、変換 (▼)、辞書登録はかな系のモードにしか無い。読みはどのモードでもひらがなで持ち、
//! 表示と確定のときにモードの文字種にする。かな系のモードで `/` を打つと、SKK の abbrev と同じく英字の見出しを打てる。
//!
//! 候補は辞書の候補、生成した表記の順に送り、それも送り切ると SKK と同じ辞書登録に入る。
//! 登録する語の入力欄も内側の [`Session`] で変換できるので、登録は入れ子になる。
//! 入力欄は外側とは別のモードを持ち、ddskk と同じくひらがなで始まる。
//! 候補の表示中に `X` を打つと、ddskk と同じく確かめてから表示中の候補を辞書から消す。
//! 候補を確定するたびに、確定のしかたによらず [`Converter::committed`] で知らせる。

mod romaji;
mod script;

use std::mem;

use romaji::Romaji;

/// 候補をその場に 1 件ずつ表示する件数。これを超えると一覧に切り替える。
pub const INLINE_CANDIDATES: usize = 4;
/// 一覧の 1 ページの選択キー。
pub const PAGE_LABELS: [char; 7] = ['a', 's', 'd', 'f', 'j', 'k', 'l'];
/// 辞書登録の入れ子の最大段数。
pub const MAX_REGISTRATION_DEPTH: usize = 16;

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

/// 変換する見出し。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Midashi {
    /// `reading` は送りがなを含まない読みで、モードによらずひらがな。
    Kana {
        reading: String,
        okuri: Option<Okuri>,
    },
    /// SKK の abbrev。`/` のあとに打った英字をそのまま見出しにする。
    Abbrev(String),
}

impl Midashi {
    /// SKK 辞書の見出しの書式。送りありは読みと送りがなの子音 (`あt`) になる。
    pub fn key(&self) -> String {
        match self {
            Midashi::Kana {
                reading,
                okuri: Some(okuri),
            } => format!("{reading}{}", okuri.consonant),
            Midashi::Kana {
                reading,
                okuri: None,
            } => reading.clone(),
            Midashi::Abbrev(text) => text.clone(),
        }
    }

    pub fn okuri(&self) -> Option<&Okuri> {
        match self {
            Midashi::Kana { okuri, .. } => okuri.as_ref(),
            Midashi::Abbrev(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Query<'a> {
    /// フロントエンドが送った左文脈に、同じキーの処理中に確定した文字列を足したもの。
    pub left_context: &'a str,
    pub midashi: &'a Midashi,
}

/// 候補はどれも送りがなを含まない。
pub trait Converter {
    /// 辞書の候補を表示する順に返す。
    fn candidates(&mut self, query: &Query) -> Vec<String>;
    /// 辞書に頼らずに表記を生成し、良い順に返す。辞書の候補を送り切ったとき (辞書に無ければ変換の開始時) に 1 度だけ呼ぶ。
    fn generate(&mut self, query: &Query) -> Vec<String>;
    /// 辞書登録で入力された語を保存する。
    fn register(&mut self, midashi: &Midashi, word: &str);
    /// 語を辞書から消す。
    fn purge(&mut self, midashi: &Midashi, word: &str);
    /// 候補を確定した。確定のしかた (Enter、次の文字、一覧からの選択、モードの切り替え、flush、辞書登録など) によらず、
    /// 確定した文字列に候補を含めるたびに 1 回呼ぶ。`word` は送りがなを含まない。
    fn committed(&mut self, midashi: &Midashi, word: &str, pick: Pick);
    /// 候補の辞書注釈を返す。注釈が無い場合や生成候補は None。
    fn annotation(&self, _midashi: &Midashi, _word: &str) -> Option<String> {
        None
    }
}

/// 確定した候補が、変換を始めたときに最初に出した候補だったか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    First,
    /// 候補を送ったか一覧から選んだもの、または辞書登録した語。
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Response {
    /// false ならフロントエンドは `commit` を挿入したあと、元のキーを通常どおり処理する。
    pub handled: bool,
    pub commit: String,
    pub preedit: String,
    /// インライン変換中に表示している候補の注釈。候補一覧の表示中や変換中でないときは None。
    pub annotation: Option<String>,
    /// 入力中の階層 (辞書登録中ならいちばん内側の入力欄) の候補一覧。
    pub candidates: Option<CandidatePage>,
    /// 辞書登録の入力欄か、辞書から消すかの問いがあるときだけ、いちばん内側のもの。
    pub minibuffer: Option<Minibuffer>,
    /// 次のキーを受ける階層 (辞書登録中ならいちばん内側の入力欄) のモード。
    pub mode: Mode,
    /// true の間は [`Key::Kakutei`] 以外のキーをすべて処理せずに返す (`handled` は false、`commit` は空)。
    /// フロントエンドは [`Key::Kakutei`] 以外のキーを送らずに、そのまま処理してよい。
    pub passthrough: bool,
    /// 見出しの入力中 (▽) に Space で変換する読み。送りがなの入力中、abbrev、辞書登録の入力欄では None。
    pub reading: Option<String>,
    /// [`Session`] が要求を処理するたびに増える。[`Session::choose`] に渡し、選んだあとに打ったキーが無いことを確かめる。
    pub version: u64,
}

/// Emacs のミニバッファに当たる 1 行。フロントエンドは `prompt` `text` `preedit` をこの順に並べて表示する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minibuffer {
    /// `[辞書登録] まよねーず: ` や `あした /明日/ を辞書から削除します。良いですか？ (yes or no) `。
    /// 辞書登録は入れ子の深さだけ括弧を重ね、見出しはモードによらずひらがな。
    pub prompt: String,
    /// 確定した文字列。
    pub text: String,
    /// 未確定文字列。
    pub preedit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateItem {
    /// 選択キー。
    pub label: char,
    /// 送りがなを含めた候補。
    pub word: String,
    /// 辞書注釈。無い場合は None。
    pub annotation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePage {
    /// 選択キーと、送りがなを含めた候補・注釈。
    pub items: Vec<CandidateItem>,
    pub page: usize,
    pub pages: usize,
}

#[derive(Debug, Default)]
pub struct Session {
    input: Input,
    version: u64,
    registration_depth: usize,
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
    RomanContinuation(char),
    Char(char),
    SetHenkanPoint,
    Purge,
    Abbrev,
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
    Abbrev {
        text: String,
    },
    Henkan {
        conv: Conversion,
        trailing_roman: Romaji,
    },
    Purge {
        conv: Conversion,
        trailing_roman: Romaji,
        answer: String,
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
struct Conversion {
    midashi: Midashi,
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
    NoCandidates(Midashi),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cursor {
    Inline(usize),
    Page(usize),
}

enum PurgeAnswer {
    Yes,
    No,
    Neither,
}

struct Step<'a> {
    left_context: &'a str,
    commit: String,
    next_mode: Option<Mode>,
    registration_depth: usize,
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
    fn with_registration_depth(registration_depth: usize) -> Self {
        Self {
            registration_depth,
            ..Self::default()
        }
    }

    pub fn handle(
        &mut self,
        key: Key,
        left_context: &str,
        converter: &mut dyn Converter,
    ) -> Response {
        self.version += 1;
        let mut step = Step {
            left_context,
            commit: String::new(),
            next_mode: None,
            registration_depth: self.registration_depth,
            converter,
        };
        let handled = match &mut self.input {
            Input::Kana(kana) => kana.step(key, &mut step),
            Input::Latin(latin) => latin.handle(key, &mut step),
        };
        if let Some(mode) = step.next_mode {
            self.input = Input::new(mode);
        }
        self.response(handled, step.commit, Some(converter))
    }

    /// 入力途中のものを表示どおりに確定し、状態を空にする。モードは変えない。フロントエンドが入力先を離れるときに使う。
    pub fn flush(&mut self, converter: &mut dyn Converter) -> Response {
        self.version += 1;
        let commit = self.take_pending(converter);
        self.response(true, commit, Some(converter))
    }

    /// [`Session::flush`] したうえで、モードを `mode` にする。
    pub fn flush_to(&mut self, mode: Mode, converter: &mut dyn Converter) -> Response {
        self.version += 1;
        let commit = self.take_pending(converter);
        self.input = Input::new(mode);
        self.response(true, commit, Some(converter))
    }

    /// 入力途中のものを確定せずに捨て、状態を空にする。モードは変えず、辞書登録中なら外側のモードに戻る。
    /// 何も確定しないので [`Converter`] を呼ばない。
    pub fn discard(&mut self) -> Response {
        self.version += 1;
        if let Input::Kana(kana) = &mut self.input {
            kana.phase = Phase::default();
        }
        self.response(true, String::new(), None)
    }

    /// 入力途中のものを確定せずに捨て、状態を空にしたうえでモードを `mode` にする。
    /// 何も確定しないので [`Converter`] を呼ばない。フロントエンドが入力先からフォーカスを失ったときなどに使う。
    pub fn discard_to(&mut self, mode: Mode) -> Response {
        self.version += 1;
        self.input = Input::new(mode);
        self.response(true, String::new(), None)
    }

    /// [`Session::flush`] したうえで、モードを始めのひらがなに戻す。フロントエンドが入力を有効にしたときに使う。
    pub fn reset(&mut self, converter: &mut dyn Converter) -> Response {
        self.flush_to(Mode::Hiragana, converter)
    }

    /// 見出しの入力中 (▽) に、[`Response::reading`] の候補から `word` を選んで確定する。Space で変換して `word` まで送って確定したのと同じに扱う。
    /// `version` が最後の応答の版でなければ (そのあとにキーを処理していれば)、何もせずに `handled` を false で返す。
    /// `word` が見出しの候補に無いときも同じ。
    pub fn choose(
        &mut self,
        version: u64,
        word: &str,
        left_context: &str,
        converter: &mut dyn Converter,
    ) -> Response {
        let fresh = version == self.version;
        self.version += 1;
        let mut step = Step {
            left_context,
            commit: String::new(),
            next_mode: None,
            registration_depth: self.registration_depth,
            converter,
        };
        let chosen = match &mut self.input {
            Input::Kana(kana) if fresh => kana.choose(word, &mut step),
            _ => false,
        };
        self.response(chosen, step.commit, Some(converter))
    }

    fn take_pending(&mut self, converter: &mut dyn Converter) -> String {
        let mut step = Step {
            left_context: "",
            commit: String::new(),
            next_mode: None,
            registration_depth: self.registration_depth,
            converter,
        };
        if let Input::Kana(kana) = &mut self.input {
            kana.flush(&mut step);
        }
        step.commit
    }

    fn response(
        &self,
        handled: bool,
        commit: String,
        converter: Option<&dyn Converter>,
    ) -> Response {
        Response {
            handled,
            commit,
            preedit: self.preedit(),
            annotation: self.annotation(converter),
            candidates: self.page(converter),
            minibuffer: self.minibuffer(1),
            mode: self.mode(),
            passthrough: matches!(self.input, Input::Latin(Latin::Ascii)),
            reading: match &self.input {
                Input::Kana(kana) => kana.reading(),
                Input::Latin(_) => None,
            },
            version: self.version,
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

    fn minibuffer(&self, depth: usize) -> Option<Minibuffer> {
        let Input::Kana(kana) = &self.input else {
            return None;
        };
        kana.minibuffer(depth)
    }

    fn annotation(&self, converter: Option<&dyn Converter>) -> Option<String> {
        let Input::Kana(kana) = &self.input else {
            return None;
        };
        kana.annotation(converter)
    }

    fn page(&self, converter: Option<&dyn Converter>) -> Option<CandidatePage> {
        let Input::Kana(kana) = &self.input else {
            return None;
        };
        kana.page(converter)
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
            Key::Char(c) if roman.continues(c) => KanaKey::RomanContinuation(c),
            Key::Char('q') => KanaKey::Switch(Switch::ToggleKatakana),
            Key::Char('l') => KanaKey::Switch(Switch::Ascii),
            Key::Char('L') => KanaKey::Switch(Switch::FullwidthLatin),
            Key::Char('Q') => KanaKey::SetHenkanPoint,
            Key::Char('X') => KanaKey::Purge,
            Key::Char('/') => KanaKey::Abbrev,
            Key::Char(c) => KanaKey::Char(c),
            Key::Space if roman.continues(' ') => KanaKey::RomanContinuation(' '),
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
            Phase::Abbrev { text } => self.abbrev(text, key, step),
            Phase::Henkan {
                conv,
                trailing_roman,
            } => self.henkan(conv, trailing_roman, key, step),
            Phase::Purge {
                conv,
                trailing_roman,
                answer,
            } => self.purge(conv, trailing_roman, answer, key, step),
            Phase::Register {
                reg,
                trailing_roman,
            } => self.register(reg, trailing_roman, key, step),
        }
    }

    fn flush(&mut self, step: &mut Step) {
        let hiragana = match mem::take(&mut self.phase) {
            Phase::Direct { mut roman } => roman.flush().to_string(),
            Phase::Midashi { kana, mut roman } => kana + roman.flush(),
            Phase::Okuri { kana, .. } => kana,
            Phase::Abbrev { text } => return step.commit.push_str(&text),
            Phase::Henkan { conv, .. } => return conv.commit_displayed(self.script, step),
            Phase::Purge { conv, .. } => return step.commit.push_str(&conv.displayed(self.script)),
            Phase::Register { reg, .. } => {
                return step.commit.push_str(&reg.midashi().typed(self.script));
            }
        };
        step.commit.push_str(&self.script.render(&hiragana));
    }

    fn direct(&mut self, mut roman: Romaji, key: Key, step: &mut Step) -> bool {
        let handled = match KanaKey::of(key, &roman) {
            KanaKey::Switch(switch) => {
                step.commit.push_str(&self.script.render(roman.flush()));
                step.next_mode = Some(self.script.after(switch));
                return true;
            }
            KanaKey::SetHenkanPoint => {
                step.commit.push_str(&self.script.render(roman.flush()));
                self.phase = Phase::Midashi {
                    kana: String::new(),
                    roman: Romaji::default(),
                };
                return true;
            }
            KanaKey::Abbrev => {
                step.commit.push_str(&self.script.render(roman.flush()));
                self.phase = Phase::Abbrev {
                    text: String::new(),
                };
                return true;
            }
            KanaKey::Purge => {
                step.commit.push_str(&self.script.render(roman.flush()));
                step.commit.push('X');
                true
            }
            KanaKey::Char(c) if c.is_ascii_uppercase() => {
                step.commit.push_str(&self.script.render(roman.flush()));
                let mut roman = Romaji::default();
                let kana = roman.feed(c.to_ascii_lowercase());
                self.phase = Phase::Midashi { kana, roman };
                return true;
            }
            KanaKey::RomanContinuation(c) | KanaKey::Char(c) => {
                step.commit.push_str(&self.script.render(&roman.feed(c)));
                true
            }
            KanaKey::Space | KanaKey::Enter => {
                step.commit.push_str(&self.script.render(roman.flush()));
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

    fn reading(&self) -> Option<String> {
        let Phase::Midashi { kana, roman } = &self.phase else {
            return None;
        };
        let reading = format!("{kana}{}", roman.clone().flush());
        (!reading.is_empty()).then_some(reading)
    }

    fn choose(&mut self, word: &str, step: &mut Step) -> bool {
        let Some(reading) = self.reading() else {
            return false;
        };
        let midashi = Midashi::Kana {
            reading,
            okuri: None,
        };
        let left_context = step.left_context_with_commit();
        let words = step.converter.candidates(&midashi.query(&left_context));
        let Some(index) = words.iter().position(|w| w == word) else {
            return false;
        };
        let pick = if index == 0 { Pick::First } else { Pick::Other };
        step.commit_candidate(&midashi, word, pick, self.script);
        self.phase = Phase::default();
        true
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

    fn start_empty_midashi(&mut self, committed_kana: &str, step: &mut Step) {
        step.commit.push_str(&self.script.render(committed_kana));
        self.phase = Phase::Midashi {
            kana: String::new(),
            roman: Romaji::default(),
        };
    }

    fn midashi(&mut self, mut kana: String, mut roman: Romaji, key: Key, step: &mut Step) -> bool {
        match KanaKey::of(key, &roman) {
            KanaKey::Switch(switch) => {
                kana.push_str(roman.flush());
                self.commit_midashi(&kana, switch, step);
                return true;
            }
            KanaKey::SetHenkanPoint => {
                kana.push_str(roman.flush());
                self.start_empty_midashi(&kana, step);
                return true;
            }
            KanaKey::Abbrev => kana.push_str(roman.flush()),
            KanaKey::Purge => {
                kana.push_str(roman.flush());
                kana.push('X');
            }
            KanaKey::Char(c) if c.is_ascii_uppercase() && !kana.is_empty() => {
                kana.push_str(roman.flush());
                let consonant = c.to_ascii_lowercase();
                self.feed_okuri(kana, consonant, Romaji::default(), consonant, step);
                return true;
            }
            KanaKey::RomanContinuation(c) => kana.push_str(&roman.feed(c)),
            KanaKey::Char(c) => kana.push_str(&roman.feed(c.to_ascii_lowercase())),
            KanaKey::Space => {
                kana.push_str(roman.flush());
                if !kana.is_empty() {
                    let midashi = Midashi::Kana {
                        reading: kana,
                        okuri: None,
                    };
                    self.convert(midashi, Romaji::default(), step);
                }
                return true;
            }
            KanaKey::Enter => {
                kana.push_str(roman.flush());
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
            KanaKey::SetHenkanPoint => self.start_empty_midashi(&kana, step),
            KanaKey::RomanContinuation(c) | KanaKey::Char(c) => {
                self.feed_okuri(kana, consonant, roman, c, step)
            }
            KanaKey::Space | KanaKey::Abbrev | KanaKey::Purge => {
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
            let midashi = Midashi::Kana {
                reading: kana,
                okuri: Some(Okuri {
                    consonant,
                    kana: okuri,
                }),
            };
            self.convert(midashi, roman, step);
        }
    }

    fn abbrev(&mut self, mut text: String, key: Key, step: &mut Step) -> bool {
        match key {
            Key::Char(c) => text.push(c),
            Key::Space if text.is_empty() => return true,
            Key::Space => {
                self.convert(Midashi::Abbrev(text), Romaji::default(), step);
                return true;
            }
            Key::Enter | Key::Kakutei => {
                step.commit.push_str(&text);
                return true;
            }
            Key::ToggleHalfwidth => {
                step.commit
                    .extend(text.chars().filter_map(script::fullwidth_latin));
                return true;
            }
            Key::Backspace => {
                if text.pop().is_none() {
                    return true;
                }
            }
            Key::Cancel => return true,
        }
        self.phase = Phase::Abbrev { text };
        true
    }

    fn convert(&mut self, midashi: Midashi, trailing_roman: Romaji, step: &mut Step) {
        let left_context = step.left_context_with_commit();
        let query = midashi.query(&left_context);
        let mut words = step.converter.candidates(&query);
        let generated = words.is_empty();
        if generated {
            push_unique(&mut words, step.converter.generate(&query));
        }
        self.phase = match Candidates::new(words) {
            Some(candidates) => Phase::Henkan {
                conv: Conversion {
                    midashi,
                    candidates,
                    generated,
                    cursor: Cursor::Inline(0),
                },
                trailing_roman,
            },
            None if step.registration_depth < MAX_REGISTRATION_DEPTH => Phase::Register {
                reg: Box::new(Registration::new(
                    Origin::NoCandidates(midashi),
                    step.registration_depth + 1,
                )),
                trailing_roman,
            },
            None => cancel_no_candidates(midashi, trailing_roman),
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
                if !conv.next()
                    && !conv.advance_to_generated(step)
                    && step.registration_depth < MAX_REGISTRATION_DEPTH
                {
                    self.phase = Phase::Register {
                        reg: Box::new(Registration::new(
                            Origin::Candidates(conv),
                            step.registration_depth + 1,
                        )),
                        trailing_roman,
                    };
                    return true;
                }
            }
            (Key::Char('x'), _) | (Key::Backspace, Cursor::Page(_)) => {
                if !conv.prev() {
                    self.phase = conv.midashi.reedit();
                    return true;
                }
            }
            (Key::Char('X'), Cursor::Inline(_)) => {
                self.phase = Phase::Purge {
                    conv,
                    trailing_roman,
                    answer: String::new(),
                };
                return true;
            }
            (Key::Cancel, _) => {
                self.phase = conv.midashi.reedit();
                return true;
            }
            (Key::Char(c), Cursor::Page(page)) => {
                if let Some(word) = conv.page_word(page, c) {
                    step.commit_candidate(&conv.midashi, word, Pick::Other, self.script);
                    self.phase = Phase::Direct {
                        roman: trailing_roman,
                    };
                    return true;
                }
            }
            (_, Cursor::Page(_)) => {}
            (Key::Enter, Cursor::Inline(_)) => {
                conv.commit_displayed(self.script, step);
                self.phase = Phase::Direct {
                    roman: trailing_roman,
                };
                return true;
            }
            // ddskk の `skk-delete-backward-char` と同じく、確定してから 1 字消す
            (Key::Backspace, Cursor::Inline(_)) => {
                conv.commit_displayed(self.script, step);
                step.commit.pop();
                self.phase = Phase::Direct {
                    roman: trailing_roman,
                };
                return true;
            }
            (Key::Char(_) | Key::Kakutei | Key::ToggleHalfwidth, Cursor::Inline(_)) => {
                conv.commit_displayed(self.script, step);
                return self.direct(trailing_roman, key, step);
            }
        }
        self.phase = Phase::Henkan {
            conv,
            trailing_roman,
        };
        true
    }

    // ddskk の `skk-purge-from-jisyo` は `yes-or-no-p` で確かめ、消したあとは表示中の候補も送りがなも消して確定する
    fn purge(
        &mut self,
        conv: Conversion,
        trailing_roman: Romaji,
        mut answer: String,
        key: Key,
        step: &mut Step,
    ) -> bool {
        match key {
            Key::Char(c) => answer.push(c),
            Key::Space => answer.push(' '),
            Key::Backspace => {
                answer.pop();
            }
            Key::Enter | Key::Kakutei => match PurgeAnswer::of(&answer) {
                PurgeAnswer::Yes => {
                    if let Some(word) = conv.inline_word() {
                        step.converter.purge(&conv.midashi, word);
                    }
                    return true;
                }
                PurgeAnswer::No => {
                    self.phase = Phase::Henkan {
                        conv,
                        trailing_roman,
                    };
                    return true;
                }
                PurgeAnswer::Neither => answer.clear(),
            },
            Key::Cancel => {
                self.phase = Phase::Henkan {
                    conv,
                    trailing_roman,
                };
                return true;
            }
            Key::ToggleHalfwidth => {}
        }
        self.phase = Phase::Purge {
            conv,
            trailing_roman,
            answer,
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
                    step.converter.register(reg.midashi(), word);
                    step.commit_candidate(reg.midashi(), word, Pick::Other, self.script);
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
            Phase::Abbrev { text } => format!("▽{text}"),
            Phase::Henkan {
                conv,
                trailing_roman,
            } => format!(
                "▼{}{}",
                conv.displayed(self.script),
                trailing_roman.pending()
            ),
            Phase::Purge { conv, .. } => format!("▼{}", conv.displayed(self.script)),
            Phase::Register { reg, .. } => format!("▼{}", reg.midashi().typed(self.script)),
        }
    }

    fn minibuffer(&self, depth: usize) -> Option<Minibuffer> {
        match &self.phase {
            Phase::Register { reg, .. } => reg.inner.minibuffer(depth + 1).or_else(|| {
                Some(Minibuffer {
                    prompt: format!(
                        "{}辞書登録{} {}: ",
                        "[".repeat(depth),
                        "]".repeat(depth),
                        reg.midashi().label()
                    ),
                    text: reg.text.clone(),
                    preedit: reg.inner.preedit(),
                })
            }),
            Phase::Purge { conv, answer, .. } => Some(Minibuffer {
                prompt: format!(
                    "{} /{}/ を辞書から削除します。良いですか？ (yes or no) ",
                    conv.midashi.key(),
                    conv.inline_word().unwrap_or_default()
                ),
                text: answer.clone(),
                preedit: String::new(),
            }),
            _ => None,
        }
    }

    fn annotation(&self, converter: Option<&dyn Converter>) -> Option<String> {
        match &self.phase {
            Phase::Henkan { conv, .. } => {
                let word = conv.inline_word()?;
                converter?.annotation(&conv.midashi, word)
            }
            Phase::Register { reg, .. } => reg.inner.annotation(converter),
            _ => None,
        }
    }

    fn page(&self, converter: Option<&dyn Converter>) -> Option<CandidatePage> {
        let conv = match &self.phase {
            Phase::Henkan { conv, .. } => conv,
            Phase::Register { reg, .. } => return reg.inner.page(converter),
            _ => return None,
        };
        let Cursor::Page(page) = conv.cursor else {
            return None;
        };
        let items = conv
            .page_words(page)
            .iter()
            .zip(PAGE_LABELS)
            .map(|(word, label)| CandidateItem {
                label,
                word: conv.midashi.surface(word, self.script),
                annotation: converter.and_then(|c| c.annotation(&conv.midashi, word)),
            })
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

    fn commit_candidate(&mut self, midashi: &Midashi, word: &str, pick: Pick, script: Script) {
        self.commit.push_str(&midashi.surface(word, script));
        self.converter.committed(midashi, word, pick);
    }
}

impl PurgeAnswer {
    fn of(answer: &str) -> Self {
        match answer {
            "yes" => PurgeAnswer::Yes,
            "no" => PurgeAnswer::No,
            _ => PurgeAnswer::Neither,
        }
    }
}

impl Midashi {
    fn query<'a>(&'a self, left_context: &'a str) -> Query<'a> {
        Query {
            left_context,
            midashi: self,
        }
    }

    fn surface(&self, word: &str, script: Script) -> String {
        let okuri = self
            .okuri()
            .map_or(String::new(), |o| script.render(&o.kana));
        format!("{word}{okuri}")
    }

    fn typed(&self, script: Script) -> String {
        match self {
            Midashi::Kana { reading, okuri } => script.render(&format!(
                "{reading}{}",
                okuri.as_ref().map_or("", |o| &o.kana)
            )),
            Midashi::Abbrev(text) => text.clone(),
        }
    }

    fn label(&self) -> String {
        match self {
            Midashi::Kana {
                reading,
                okuri: Some(okuri),
            } => format!("{reading}*{}", okuri.kana),
            Midashi::Kana {
                reading,
                okuri: None,
            } => reading.clone(),
            Midashi::Abbrev(text) => text.clone(),
        }
    }

    fn reedit(self) -> Phase {
        match self {
            Midashi::Kana { reading, .. } => Phase::Midashi {
                kana: reading,
                roman: Romaji::default(),
            },
            Midashi::Abbrev(text) => Phase::Abbrev { text },
        }
    }
}

impl Registration {
    fn new(origin: Origin, depth: usize) -> Self {
        Self {
            origin,
            text: String::new(),
            inner: Session::with_registration_depth(depth),
        }
    }

    fn midashi(&self) -> &Midashi {
        match &self.origin {
            Origin::Candidates(conv) => &conv.midashi,
            Origin::NoCandidates(midashi) => midashi,
        }
    }

    fn field_left_context(&self, outer_left_context: &str) -> String {
        format!("{outer_left_context}{}", self.text)
    }

    fn word_without_okuri(&self) -> &str {
        self.midashi()
            .okuri()
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
            Origin::NoCandidates(midashi) => cancel_no_candidates(midashi, trailing_roman),
        }
    }
}

fn cancel_no_candidates(midashi: Midashi, trailing_roman: Romaji) -> Phase {
    match midashi {
        Midashi::Kana { reading, okuri } => Phase::Midashi {
            kana: format!("{reading}{}", okuri.map_or(String::new(), |o| o.kana)),
            roman: trailing_roman,
        },
        Midashi::Abbrev(text) => Phase::Abbrev { text },
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
            Cursor::Inline(i) => self.midashi.surface(&self.candidates[i], script),
            Cursor::Page(_) => self.midashi.typed(script),
        }
    }

    fn inline_word(&self) -> Option<&str> {
        match self.cursor {
            Cursor::Inline(i) => Some(&self.candidates[i]),
            Cursor::Page(_) => None,
        }
    }

    fn commit_displayed(&self, script: Script, step: &mut Step) {
        match self.cursor {
            Cursor::Inline(i) => {
                let pick = if i == 0 { Pick::First } else { Pick::Other };
                step.commit_candidate(&self.midashi, &self.candidates[i], pick, script);
            }
            Cursor::Page(_) => step.commit.push_str(&self.midashi.typed(script)),
        }
    }

    fn advance_to_generated(&mut self, step: &mut Step) -> bool {
        if self.generated {
            return false;
        }
        self.generated = true;
        let shown_before = self.shown();
        let left_context = step.left_context_with_commit();
        let words = step.converter.generate(&self.midashi.query(&left_context));
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

    fn page_word(&self, page: usize, label: char) -> Option<&str> {
        let index = PAGE_LABELS.iter().position(|&l| l == label)?;
        self.page_words(page).get(index).map(String::as_str)
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
