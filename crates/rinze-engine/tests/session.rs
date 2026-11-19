use std::collections::HashMap;

use rinze_engine::{CandidatePage, Converter, Key, Query, Response, Session};

/// 見出しから候補を引くだけの変換器。受け取った問い合わせを記録する。
/// 左文脈、読み、送りがな (子音とかな)
type Recorded = (String, String, Option<(char, String)>);

#[derive(Default)]
struct Table {
    entries: HashMap<String, Vec<String>>,
    queries: Vec<Recorded>,
}

impl Table {
    fn new(entries: &[(&str, &[&str])]) -> Self {
        Self {
            entries: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
                .collect(),
            queries: Vec::new(),
        }
    }
}

impl Converter for Table {
    fn candidates(&mut self, q: &Query) -> Vec<String> {
        let okuri = q.okuri.map(|o| (o.consonant, o.kana.clone()));
        self.queries.push((
            q.left_context.to_string(),
            q.reading.to_string(),
            okuri.clone(),
        ));
        let key = match okuri {
            Some((c, _)) => format!("{}{c}", q.reading),
            None => q.reading.to_string(),
        };
        self.entries.get(&key).cloned().unwrap_or_default()
    }
}

/// フロントエンドの代わり。確定文字列をバッファへ挿入し、エンジンが処理しなかったキーは自分で処理する。
struct Editor {
    session: Session,
    table: Table,
    buffer: String,
    last: Response,
}

impl Editor {
    fn new(table: Table) -> Self {
        Self {
            session: Session::default(),
            table,
            buffer: String::new(),
            last: Response::default(),
        }
    }

    /// `<Space>` `<CR>` `<BS>` `<C-g>` 以外の文字はそのまま 1 キーとして打つ。
    fn type_keys(&mut self, keys: &str) -> &mut Self {
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            let (key, len) = [
                ("<Space>", Key::Space),
                ("<CR>", Key::Enter),
                ("<BS>", Key::Backspace),
                ("<C-g>", Key::Cancel),
            ]
            .into_iter()
            .find(|(name, _)| rest.starts_with(name))
            .map_or((Key::Char(c), c.len_utf8()), |(name, key)| {
                (key, name.len())
            });
            rest = &rest[len..];
            self.press(key);
        }
        self
    }

    fn press(&mut self, key: Key) {
        let res = self.session.handle(key, &self.buffer, &mut self.table);
        self.buffer.push_str(&res.commit);
        if !res.handled {
            match key {
                Key::Space => self.buffer.push(' '),
                Key::Enter => self.buffer.push('\n'),
                Key::Backspace => {
                    self.buffer.pop();
                }
                Key::Char(c) => self.buffer.push(c),
                Key::Cancel => {}
            }
        }
        self.last = res;
    }

    fn preedit(&self) -> &str {
        &self.last.preedit
    }

    fn page(&self) -> Option<&CandidatePage> {
        self.last.candidates.as_ref()
    }
}

fn okuri(c: char, kana: &str) -> Option<(char, String)> {
    Some((c, kana.to_string()))
}

#[test]
fn 明日会えますか() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日"]), ("あe", &["会", "和"])]));
    ed.type_keys("Asita");
    assert_eq!(ed.preedit(), "▽あした");
    ed.type_keys("<Space>");
    assert_eq!(ed.preedit(), "▼明日");
    ed.type_keys("AE");
    assert_eq!(ed.buffer, "明日", "大文字で前の候補が暗黙に確定する");
    assert_eq!(ed.preedit(), "▼会え");
    ed.type_keys("masuka");
    assert_eq!(ed.buffer, "明日会えますか");
    assert_eq!(ed.preedit(), "");
    assert_eq!(
        ed.table.queries[1],
        ("明日".to_string(), "あ".to_string(), okuri('e', "え"))
    );
}

#[test]
fn マヨネーズを和えます() {
    let mut ed = Editor::new(Table::new(&[
        ("まよねーず", &["マヨネーズ"]),
        ("あe", &["和", "会"]),
    ]));
    ed.type_keys("Mayone-zu<Space>woAEmasu");
    assert_eq!(ed.buffer, "マヨネーズを和えます");
    assert_eq!(ed.table.queries[1].0, "マヨネーズを");
}

#[test]
fn 促音の送りがなで変換が始まり_候補を送って確定する() {
    let mut ed = Editor::new(Table::new(&[("あt", &["会", "合", "有"])]));
    ed.type_keys("koredeAT");
    assert_eq!(ed.buffer, "これで");
    assert_eq!(ed.preedit(), "▽あ*t");
    assert!(
        ed.table.queries.is_empty(),
        "送りがなが揃うまでは変換しない"
    );

    ed.type_keys("t");
    assert_eq!(ed.preedit(), "▼会っt");
    assert_eq!(
        ed.table.queries,
        [("これで".to_string(), "あ".to_string(), okuri('t', "っ"))]
    );

    ed.type_keys("<Space>");
    assert_eq!(ed.preedit(), "▼合っt");
    ed.type_keys("eimasuka?");
    assert_eq!(ed.buffer, "これで合っていますか？");
    assert_eq!(ed.preedit(), "");
}

#[test]
fn nは子音の前と確定時にんになる() {
    let mut ed = Editor::new(Table::new(&[("かん", &["缶"]), ("ほん", &["本"])]));
    ed.type_keys("kanjikon'yanyo");
    assert_eq!(ed.buffer, "かんじこんやにょ");

    ed.type_keys("Kan<Space>");
    assert_eq!(
        ed.preedit(),
        "▼缶",
        "見出しの末尾の n は変換時に「ん」になる"
    );
    ed.type_keys("<CR>");
    assert_eq!(ed.buffer, "かんじこんやにょ缶");

    ed.type_keys("hon");
    assert_eq!(ed.preedit(), "n");
    ed.type_keys("<CR>");
    assert_eq!(ed.buffer, "かんじこんやにょ缶ほん\n");
}

#[test]
fn c_g_は入力途中のものを取り消す() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日"]), ("あt", &["合"])]));
    ed.type_keys("Asita<C-g>");
    assert_eq!((ed.buffer.as_str(), ed.preedit()), ("", ""));

    ed.type_keys("Asita<Space>");
    assert_eq!(ed.preedit(), "▼明日");
    ed.type_keys("<C-g>");
    assert_eq!(
        ed.preedit(),
        "▽あした",
        "変換中の取り消しは見出しの入力に戻る"
    );
    ed.type_keys("<C-g>");
    assert_eq!((ed.buffer.as_str(), ed.preedit()), ("", ""));

    ed.type_keys("ATt<C-g>");
    assert_eq!(
        ed.preedit(),
        "▽あ",
        "送りありの変換の取り消しは送りがなも捨てる"
    );
    ed.type_keys("<C-g>k<C-g>");
    assert_eq!((ed.buffer.as_str(), ed.preedit()), ("", ""));
}

#[test]
fn 候補の表示中に次の文字を打つと確定する() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日", "朝"])]));
    ed.type_keys("Asita<Space><Space>wo");
    assert_eq!(ed.buffer, "朝を");
    ed.type_keys("Asita<Space>.");
    assert_eq!(ed.buffer, "朝を明日。");
}

#[test]
fn 空白はかなモードでは素通しする() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("a<Space>i");
    assert_eq!(ed.buffer, "あ い");
}

const KANJI: &[&str] = &[
    "漢字", "感じ", "幹事", "監事", "換字", "莞爾", "寛治", "完治", "官寺", "患児", "幹二", "勘治",
];

#[test]
fn 四回目までその場に出し五回目から一覧にする() {
    let mut ed = Editor::new(Table::new(&[("かんじ", KANJI)]));
    ed.type_keys("Kanji<Space>");
    assert_eq!(ed.preedit(), "▼漢字");
    ed.type_keys("<Space><Space><Space>");
    assert_eq!(ed.preedit(), "▼監事");
    assert_eq!(ed.page(), None);

    ed.type_keys("<Space>");
    let page = ed.page().expect("5 回目の Space で一覧を出す");
    assert_eq!((page.page, page.pages), (0, 2));
    assert_eq!(
        page.items,
        [
            ('a', "換字"),
            ('s', "莞爾"),
            ('d', "寛治"),
            ('f', "完治"),
            ('j', "官寺"),
            ('k', "患児"),
            ('l', "幹二")
        ]
        .map(|(l, w)| (l, w.to_string()))
    );

    ed.type_keys("<Space>");
    let page = ed.page().unwrap();
    assert_eq!((page.page, page.items.len()), (1, 1));
    ed.type_keys("<Space>");
    assert_eq!(
        ed.page().unwrap().page,
        1,
        "候補が尽きたら最後のページに留まる"
    );

    ed.type_keys("xx");
    assert_eq!(ed.preedit(), "▼監事", "x で前の候補に戻る");
    ed.type_keys("<Space>q");
    assert!(ed.page().is_some(), "一覧の選択キー以外は無視する");
    ed.type_keys("s");
    assert_eq!(ed.buffer, "莞爾");
    assert_eq!((ed.preedit(), ed.page()), ("", None));
}

#[test]
fn 送りありは変換開始が一件目になる() {
    let mut ed = Editor::new(Table::new(&[(
        "あt",
        &["会", "合", "有", "遭", "逢", "在"],
    )]));
    ed.type_keys("ATt<Space><Space><Space>");
    assert_eq!(ed.preedit(), "▼遭っt");
    ed.type_keys("<Space>");
    let page = ed.page().unwrap();
    assert_eq!(
        page.items,
        [('a', "逢っ".to_string()), ('s', "在っ".to_string())]
    );
    assert_eq!(ed.preedit(), "▼あっt");
    ed.type_keys("se");
    assert_eq!(ed.buffer, "在って");
}

#[test]
fn 最初の候補で_x_を打つと見出しに戻る() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日"])]));
    ed.type_keys("Asita<Space>x");
    assert_eq!(ed.preedit(), "▽あした");
}

#[test]
fn 候補が無ければ見出しのまま残り_enter_でかなを確定する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Rinze<Space>");
    assert_eq!(ed.preedit(), "▽りんぜ");
    ed.type_keys("<CR>");
    assert_eq!((ed.buffer.as_str(), ed.preedit()), ("りんぜ", ""));
}

#[test]
fn 後退で見出しを削る() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Asik<BS>");
    assert_eq!(ed.preedit(), "▽あし");
    ed.type_keys("<BS><BS>");
    assert_eq!(ed.preedit(), "▽");
    ed.type_keys("<BS>");
    assert_eq!(ed.preedit(), "");
    ed.type_keys("a<BS>");
    assert_eq!(ed.buffer, "", "未確定が無ければ後退は素通しする");
}

#[test]
fn flush_は表示どおりに確定する() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日"])]));
    ed.type_keys("Asita<Space>");
    let res = ed.session.flush();
    assert_eq!((res.commit.as_str(), res.preedit.as_str()), ("明日", ""));
    ed.type_keys("Asi");
    assert_eq!(ed.session.flush().commit, "あし");
}
