//! SKK 方式の入力の状態機械。入出力を持たず、候補の取得は [`Converter`] に任せる。
//!
//! 1 つの [`Session`] が 1 つの入力先 (フロントエンドの 1 接続) の入力途中の状態を持つ。
//! フロントエンドは [`Session::handle`] にキーと左文脈を渡し、[`Response`] の確定文字列を挿入し、
//! 未確定文字列と候補一覧を表示する。

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

pub trait Converter {
    /// 表示する順に並べた候補を返す。候補は送りがなを含まない。
    fn candidates(&mut self, query: &Query) -> Vec<String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Response {
    /// false ならフロントエンドは `commit` を挿入したあと、元のキーを通常どおり処理する。
    pub handled: bool,
    pub commit: String,
    pub preedit: String,
    pub candidates: Option<CandidatePage>,
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
    /// 未確定のローマ字。`Okuri` では送りがなの、`Henkan` では変換の後ろに続く入力のローマ字になる。
    roman: Romaji,
    phase: Phase,
}

#[derive(Debug, Default)]
enum Phase {
    /// SKK の ■ モード。かなは打つそばから確定する。
    #[default]
    Direct,
    /// ▽ モード。見出しを入力中。
    Midashi { kana: String },
    /// ▽ モードで送りがなの開始を打ち、送りがなのかなが揃うのを待っている。
    Okuri { kana: String, consonant: char },
    /// ▼ モード。候補を表示中。
    Henkan(Conversion),
}

#[derive(Debug)]
struct Conversion {
    reading: String,
    okuri: Option<Okuri>,
    /// 空にならない。
    candidates: Vec<String>,
    cursor: Cursor,
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
        let left_context = format!("{}{}", step.left_context, step.commit);
        let candidates = step.converter.candidates(&Query {
            left_context: &left_context,
            reading: &reading,
            okuri: okuri.as_ref(),
        });
        self.phase = if candidates.is_empty() {
            // 辞書登録はまだ無いので、候補が無ければ見出しの入力に戻して、かなのまま確定できるようにする
            let kana = reading + okuri.as_ref().map_or("", |o| &o.kana);
            Phase::Midashi { kana }
        } else {
            Phase::Henkan(Conversion {
                reading,
                okuri,
                candidates,
                cursor: Cursor::Inline(0),
            })
        };
    }

    fn henkan(&mut self, mut conv: Conversion, key: Key, step: &mut Step) -> bool {
        match (key, conv.cursor) {
            (Key::Space, _) => conv.next(),
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
        }
    }

    fn page(&self) -> Option<CandidatePage> {
        let Phase::Henkan(conv) = &self.phase else {
            return None;
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

impl Conversion {
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

    /// 候補が尽きたら何もしない (辞書登録はまだ無い)。
    fn next(&mut self) {
        let n = self.candidates.len();
        self.cursor = match self.cursor {
            Cursor::Inline(i) if i + 1 < n.min(INLINE_CANDIDATES) => Cursor::Inline(i + 1),
            Cursor::Inline(i) if i + 1 == INLINE_CANDIDATES && n > INLINE_CANDIDATES => {
                Cursor::Page(0)
            }
            Cursor::Page(p) if p + 1 < self.pages() => Cursor::Page(p + 1),
            cursor => cursor,
        };
    }

    /// 最初の候補より前に戻ろうとしたら false を返す。
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
