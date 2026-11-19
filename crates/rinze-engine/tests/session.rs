use std::collections::HashMap;

use rinze_engine::{
    CandidatePage, Converter, Key, Mode, Okuri, Query, RegistrationLine, Response, Session,
};

#[derive(Debug, PartialEq)]
struct Recorded {
    left_context: String,
    reading: String,
    okuri: Option<Okuri>,
}

fn recorded(left_context: &str, reading: &str, okuri: Option<Okuri>) -> Recorded {
    Recorded {
        left_context: left_context.to_string(),
        reading: reading.to_string(),
        okuri,
    }
}

#[derive(Debug, PartialEq)]
struct Registered {
    midashi: String,
    word: String,
}

fn registered(midashi: &str, word: &str) -> Registered {
    Registered {
        midashi: midashi.to_string(),
        word: word.to_string(),
    }
}

#[derive(Default)]
struct Table {
    entries: HashMap<String, Vec<String>>,
    generated: HashMap<String, Vec<String>>,
    queries: Vec<Recorded>,
    generations: Vec<Recorded>,
    registered: Vec<Registered>,
}

fn table(entries: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect()
}

fn midashi(reading: &str, okuri: Option<&Okuri>) -> String {
    match okuri {
        Some(o) => format!("{reading}{}", o.consonant),
        None => reading.to_string(),
    }
}

fn record(q: &Query) -> Recorded {
    recorded(q.left_context, q.reading, q.okuri.cloned())
}

impl Table {
    fn new(entries: &[(&str, &[&str])]) -> Self {
        Self {
            entries: table(entries),
            ..Self::default()
        }
    }

    fn generating(mut self, generated: &[(&str, &[&str])]) -> Self {
        self.generated = table(generated);
        self
    }
}

impl Converter for Table {
    fn candidates(&mut self, q: &Query) -> Vec<String> {
        self.queries.push(record(q));
        let key = midashi(q.reading, q.okuri);
        self.entries.get(&key).cloned().unwrap_or_default()
    }

    fn generate(&mut self, q: &Query) -> Vec<String> {
        self.generations.push(record(q));
        let key = midashi(q.reading, q.okuri);
        self.generated.get(&key).cloned().unwrap_or_default()
    }

    fn register(&mut self, reading: &str, okuri: Option<&Okuri>, word: &str) {
        let key = midashi(reading, okuri);
        self.registered.push(registered(&key, word));
        self.entries
            .entry(key)
            .or_default()
            .insert(0, word.to_string());
    }
}

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

    fn type_keys(&mut self, keys: &str) -> &mut Self {
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            let (key, len) = [
                ("<Space>", Key::Space),
                ("<CR>", Key::Enter),
                ("<BS>", Key::Backspace),
                ("<C-g>", Key::Cancel),
                ("<C-j>", Key::Kakutei),
                ("<C-q>", Key::ToggleHalfwidth),
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
                Key::Cancel | Key::Kakutei | Key::ToggleHalfwidth => {}
            }
        }
        self.last = res;
    }

    fn preedit(&self) -> &str {
        &self.last.preedit
    }

    fn mode(&self) -> Mode {
        self.last.mode
    }

    fn page(&self) -> Option<&CandidatePage> {
        self.last.candidates.as_ref()
    }

    fn registration(&self) -> Option<String> {
        self.last.registration.as_ref().map(
            |RegistrationLine {
                 prompt,
                 text,
                 preedit,
             }| format!("{prompt}{text}{preedit}"),
        )
    }
}

fn okuri(consonant: char, kana: &str) -> Option<Okuri> {
    Some(Okuri {
        consonant,
        kana: kana.to_string(),
    })
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
        recorded("明日", "あ", okuri('e', "え"))
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
    assert_eq!(ed.table.queries[1].left_context, "マヨネーズを");
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
        [recorded("これで", "あ", okuri('t', "っ"))]
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
        ed.registration().as_deref(),
        Some("[辞書登録] かんじ: "),
        "辞書の候補も生成もなければ登録に入る"
    );
    ed.type_keys("<C-g>");
    assert_eq!(
        ed.page().unwrap().page,
        1,
        "登録をやめると最後のページに戻る"
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
fn 候補が無ければ登録に入り_やめると見出しに戻って_enter_でかなを確定する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Rinze<Space>");
    assert_eq!(ed.preedit(), "▼りんぜ");
    assert_eq!(ed.registration().as_deref(), Some("[辞書登録] りんぜ: "));
    assert_eq!(
        ed.table.generations.len(),
        1,
        "辞書に無ければ変換の開始時に生成する"
    );
    ed.type_keys("<C-g>");
    assert_eq!((ed.preedit(), ed.registration()), ("▽りんぜ", None));
    ed.type_keys("<CR>");
    assert_eq!((ed.buffer.as_str(), ed.preedit()), ("りんぜ", ""));

    ed.type_keys("Rinze<Space><CR>");
    assert_eq!(
        (ed.preedit(), ed.registration()),
        ("▽りんぜ", None),
        "空のまま Enter しても登録をやめる"
    );
    assert!(ed.table.registered.is_empty());
}

#[test]
fn 辞書の候補を送り切ると生成した表記に進み_重複は出さない() {
    let mut ed = Editor::new(
        Table::new(&[("あした", &["明日", "朝"])])
            .generating(&[("あした", &["明日", "アシタ", "あした。"])]),
    );
    ed.type_keys("kyouhaAsita<Space>");
    assert!(
        ed.table.generations.is_empty(),
        "辞書に候補があれば変換の開始時には生成しない"
    );
    ed.type_keys("<Space>");
    assert_eq!(ed.preedit(), "▼朝");
    ed.type_keys("<Space>");
    assert_eq!(ed.preedit(), "▼アシタ", "辞書と重複する「明日」は飛ばす");
    assert_eq!(
        ed.table.generations,
        [recorded("きょうは", "あした", None)],
        "生成にも左文脈を渡す"
    );
    ed.type_keys("<Space>");
    assert_eq!(ed.preedit(), "▼あした。");
    ed.type_keys("xx");
    assert_eq!(ed.preedit(), "▼朝", "生成した表記から辞書の候補へ戻れる");
    ed.type_keys("<Space><Space><Space>");
    assert!(
        ed.registration().is_some(),
        "生成した表記も送り切ると登録に入る"
    );
    assert_eq!(ed.table.generations.len(), 1, "生成は 1 度だけ");
}

#[test]
fn 辞書に無い読みは生成した表記が最初の候補になる() {
    let mut ed = Editor::new(Table::new(&[]).generating(&[("まよねーず", &["マヨネーズ"])]));
    ed.type_keys("Mayone-zu<Space>");
    assert_eq!(ed.preedit(), "▼マヨネーズ");
    ed.type_keys("wo");
    assert_eq!(ed.buffer, "マヨネーズを");
}

#[test]
fn 一覧の最後のページに空きがあれば生成した表記をそのページに足す() {
    let mut ed =
        Editor::new(Table::new(&[("かんじ", &KANJI[..6])]).generating(&[("かんじ", &["カンジ"])]));
    ed.type_keys("Kanji<Space><Space><Space><Space><Space>");
    assert_eq!(ed.page().unwrap().items.len(), 2);
    ed.type_keys("<Space>");
    let page = ed.page().unwrap();
    assert_eq!((page.page, page.pages), (0, 1));
    assert_eq!(page.items[2], ('d', "カンジ".to_string()));
}

#[test]
fn 登録した語を元の位置に確定し_次の変換で候補に出る() {
    let mut ed = Editor::new(Table::new(&[("りん", &["林"]), ("ぜ", &["是"])]));
    ed.type_keys("korehaRinze<Space>");
    assert_eq!(ed.registration().as_deref(), Some("[辞書登録] りんぜ: "));
    ed.type_keys("Rin<Space>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] りんぜ: ▼林"),
        "入力欄でも変換できる"
    );
    assert_eq!(
        ed.table.queries[1].left_context, "これは",
        "入力欄の変換の左文脈は元の位置の左文脈"
    );
    ed.type_keys("Ze<Space>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] りんぜ: 林▼是")
    );
    ed.type_keys("<CR>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] りんぜ: 林是")
    );
    assert_eq!(ed.buffer, "これは", "入力欄の確定は元の位置には入らない");
    ed.type_keys("<CR>");
    assert_eq!(
        (ed.buffer.as_str(), ed.preedit(), ed.registration()),
        ("これは林是", "", None)
    );
    assert_eq!(ed.table.registered, [registered("りんぜ", "林是")]);

    ed.type_keys("Rinze<Space>");
    assert_eq!(ed.preedit(), "▼林是");
}

#[test]
fn 登録は入れ子にでき_内側をやめると外側の入力欄に戻る() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Mayone-zu<Space>");
    ed.type_keys("Mayo<Space>");
    assert_eq!(ed.registration().as_deref(), Some("[[辞書登録]] まよ: "));
    ed.type_keys("<C-g>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] まよねーず: ▽まよ"),
        "内側の登録をやめると外側の入力欄の見出しに戻る"
    );
    ed.type_keys("<C-g>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] まよねーず: ")
    );

    ed.type_keys("Mayo<Space>");
    ed.type_keys("真夜<CR>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] まよねーず: 真夜"),
        "内側で登録した語は外側の入力欄に入る"
    );
    ed.type_keys("ne-zu<CR>");
    assert_eq!(ed.buffer, "真夜ねーず");
    assert_eq!(
        ed.table.registered,
        [
            registered("まよ", "真夜"),
            registered("まよねーず", "真夜ねーず")
        ]
    );
}

#[test]
fn 入力欄の後退と_c_g_は入力途中があればそれを先に扱う() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Mayone-zu<Space>mayo<BS>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] まよねーず: ま")
    );
    ed.type_keys("Yo<C-g>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] まよねーず: ま")
    );
    ed.type_keys("<C-g>");
    assert_eq!((ed.preedit(), ed.registration()), ("▽まよねーず", None));
    assert!(ed.table.registered.is_empty());
}

#[test]
fn 送りありの語を登録する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("AE");
    assert_eq!(ed.preedit(), "▼あえ");
    assert_eq!(ed.registration().as_deref(), Some("[辞書登録] あ*え: "));
    ed.type_keys("和<CR>");
    assert_eq!((ed.buffer.as_str(), ed.preedit()), ("和え", ""));

    ed.type_keys("AWa");
    ed.type_keys("合わ<CR>");
    assert_eq!(
        ed.buffer, "和え合わ",
        "送りがなまで打っても送りがなを重ねない"
    );
    assert_eq!(
        ed.table.registered,
        [registered("あe", "和"), registered("あw", "合")]
    );
    ed.type_keys("AE");
    assert_eq!(ed.preedit(), "▼和え");
}

#[test]
fn 登録中の_flush_は見出しをかなで確定する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Rinze<Space>ri");
    let res = ed.session.flush();
    assert_eq!((res.commit.as_str(), res.registration), ("りんぜ", None));
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

#[test]
fn q_でひらがなとカタカナを切り替える() {
    let mut ed = Editor::new(Table::new(&[]));
    assert_eq!(ed.mode(), Mode::Hiragana, "始めはひらがな");
    ed.type_keys("aq");
    assert_eq!(ed.mode(), Mode::Katakana);
    ed.type_keys("kyouq");
    assert_eq!(ed.mode(), Mode::Hiragana);
    ed.type_keys("a");
    assert_eq!(ed.buffer, "あキョウあ");
}

#[test]
fn 切り替えのキーより入力途中のローマ字の続きを優先する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("zl");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("→", Mode::Hiragana),
        "zl は矢印"
    );
    ed.type_keys("kanq");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("→かん", Mode::Katakana),
        "保留中の n は「ん」にして切り替える"
    );
    ed.type_keys("kl");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("→かん", Mode::Ascii),
        "続かない子音は捨てる"
    );
}

#[test]
fn 見出しの入力中の_q_は見出しの文字種を変えて確定する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Akanq");
    assert_eq!(
        (ed.buffer.as_str(), ed.preedit(), ed.mode()),
        ("アカン", "", Mode::Hiragana),
        "ひらがなモードではカタカナにし、モードは変えない"
    );
    ed.type_keys("qAsi");
    assert_eq!(ed.preedit(), "▽アシ");
    ed.type_keys("q");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("アカンあし", Mode::Katakana),
        "カタカナモードではひらがなにする"
    );
    ed.type_keys("ATq");
    assert_eq!(
        ed.buffer, "アカンあしあ",
        "送りがなの途中では送りがなを捨てる"
    );
}

#[test]
fn c_q_で半角カナに切り替え_もう一度で_ひらがなに戻る() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("<C-q>");
    assert_eq!(ed.mode(), Mode::HalfwidthKatakana);
    ed.type_keys("gapa-n,");
    assert_eq!(ed.buffer, "ｶﾞﾊﾟｰﾝ､", "濁点と半濁点は 2 文字になる");
    ed.type_keys("<C-q>a");
    assert_eq!((ed.buffer.as_str(), ed.mode()), ("ｶﾞﾊﾟｰﾝ､あ", Mode::Hiragana));
    ed.type_keys("q<C-q>");
    assert_eq!(
        ed.mode(),
        Mode::HalfwidthKatakana,
        "カタカナからも半角カナに入る"
    );
    ed.type_keys("q");
    assert_eq!(ed.mode(), Mode::Hiragana, "半角カナの q はひらがなに戻る");
}

#[test]
fn 見出しの入力中の_c_q_は見出しを半角カナにして確定する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Asi<C-q>");
    assert_eq!((ed.buffer.as_str(), ed.mode()), ("ｱｼ", Mode::Hiragana));
    ed.type_keys("<C-q>Kana");
    assert_eq!(ed.preedit(), "▽ｶﾅ");
    ed.type_keys("q");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("ｱｼｶﾅ", Mode::HalfwidthKatakana),
        "半角カナモードの ▽ の q も半角カナで確定する"
    );
}

#[test]
fn l_で_ascii_に入り_c_j_以外のキーを通す() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日"])]));
    ed.type_keys("al");
    assert_eq!(ed.mode(), Mode::Ascii);
    assert!(ed.last.passthrough);
    for key in [
        "q", "L", "l", "A", "<C-q>", "<C-g>", "<Space>", "<CR>", "<BS>",
    ] {
        ed.type_keys(key);
        assert_eq!(
            (ed.last.handled, ed.last.commit.as_str(), ed.mode()),
            (false, "", Mode::Ascii),
            "{key} を通す"
        );
    }
    ed.type_keys("<C-j>");
    assert_eq!(
        (ed.last.handled, ed.last.passthrough, ed.mode()),
        (true, false, Mode::Hiragana)
    );
    ed.type_keys("Asita<Space><CR>");
    assert_eq!(ed.buffer, "あqLlA 明日", "Enter と後退も通す");
}

#[test]
fn l_を全角英字に変える() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("Lq Ab-1~");
    assert_eq!(ed.mode(), Mode::FullwidthLatin);
    assert!(!ed.last.passthrough);
    assert_eq!(ed.buffer, "ｑ　Ａｂ−１〜");
    ed.type_keys("<CR><C-j>a");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("ｑ　Ａｂ−１〜\nあ", Mode::Hiragana),
        "Enter は通し、C-j でひらがなに戻る"
    );
    ed.type_keys("L<C-q>a");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("ｑ　Ａｂ−１〜\nあア", Mode::Katakana),
        "ddskk と同じく全英の C-q はカタカナに入る"
    );
}

#[test]
fn c_j_はかなのモードでは入力途中のものを確定してモードを保つ() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日"])]));
    ed.type_keys("qAsi<C-j>");
    assert_eq!((ed.buffer.as_str(), ed.mode()), ("アシ", Mode::Katakana));
    ed.type_keys("Asita<Space><C-j>");
    assert_eq!(ed.buffer, "アシ明日");
    ed.type_keys("k<C-j>");
    assert_eq!(
        (ed.buffer.as_str(), ed.preedit(), ed.last.handled),
        ("アシ明日", "", true),
        "入力途中が無くても C-j は通さない"
    );
}

#[test]
fn 候補の表示中に切り替えのキーを打つと候補を確定してから切り替える() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日", "朝"])]));
    ed.type_keys("Asita<Space>q");
    assert_eq!((ed.buffer.as_str(), ed.mode()), ("明日", Mode::Katakana));
    ed.type_keys("Asita<Space><Space>l");
    assert_eq!((ed.buffer.as_str(), ed.mode()), ("明日朝", Mode::Ascii));
    ed.type_keys("<C-j>Asita<Space>L");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("明日朝明日", Mode::FullwidthLatin)
    );
    ed.type_keys("<C-j>Asita<Space><C-q>");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("明日朝明日明日", Mode::HalfwidthKatakana)
    );
}

#[test]
fn 一覧の選択キーの_l_は候補を選ぶ() {
    let mut ed = Editor::new(Table::new(&[("かんじ", KANJI)]));
    ed.type_keys("Kanji<Space><Space><Space><Space><Space>l");
    assert_eq!((ed.buffer.as_str(), ed.mode()), ("幹二", Mode::Hiragana));
}

#[test]
fn カタカナモードでも見出しをひらがなで引き_送りがなはカタカナで確定する() {
    let mut ed = Editor::new(Table::new(&[("あした", &["明日"]), ("あe", &["会", "和"])]));
    ed.type_keys("qAsita");
    assert_eq!(ed.preedit(), "▽アシタ");
    ed.type_keys("<Space>");
    assert_eq!(ed.preedit(), "▼明日");
    assert_eq!(ed.table.queries[0], recorded("", "あした", None));
    ed.type_keys("<C-g>");
    assert_eq!(ed.preedit(), "▽アシタ");
    ed.type_keys("<Space><CR>");
    ed.type_keys("AE");
    assert_eq!(ed.preedit(), "▼会エ");
    assert_eq!(
        ed.table.queries[2],
        recorded("明日", "あ", okuri('e', "え")),
        "左文脈、読み、送りがなはひらがなで渡す"
    );
    ed.type_keys("<Space>");
    assert_eq!(ed.preedit(), "▼和エ");
    ed.type_keys("ru");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("明日和エル", Mode::Katakana)
    );
}

#[test]
fn 半角カナモードでも変換でき_送りがなは半角カナで確定する() {
    let mut ed = Editor::new(Table::new(&[("あt", &["会", "合", "有", "遭", "逢"])]));
    ed.type_keys("<C-q>AT");
    assert_eq!(ed.preedit(), "▽ｱ*t");
    ed.type_keys("t");
    assert_eq!(ed.preedit(), "▼会ｯt");
    assert_eq!(ed.table.queries[0], recorded("", "あ", okuri('t', "っ")));
    ed.type_keys("<Space><Space><Space><Space>");
    assert_eq!(
        ed.page().unwrap().items,
        [('a', "逢ｯ".to_string())],
        "一覧の候補にも半角カナの送りがなを付ける"
    );
    assert_eq!(ed.preedit(), "▼ｱｯt");
    ed.type_keys("ae");
    assert_eq!(ed.buffer, "逢ｯﾃ");
}

#[test]
fn 辞書登録の入力欄はひらがなで始まり_外側のモードとは別に切り替わる() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("qRinze<Space>");
    assert_eq!(ed.preedit(), "▼リンゼ");
    assert_eq!(
        (ed.registration().as_deref(), ed.mode()),
        (Some("[辞書登録] りんぜ: "), Mode::Hiragana),
        "見出しはひらがなで出し、入力欄はひらがなで始まる"
    );
    ed.type_keys("aqa");
    assert_eq!(
        (ed.registration().as_deref(), ed.mode()),
        (Some("[辞書登録] りんぜ: あア"), Mode::Katakana)
    );
    ed.type_keys("lb");
    assert_eq!(
        (ed.registration().as_deref(), ed.mode(), ed.last.passthrough),
        (Some("[辞書登録] りんぜ: あアb"), Mode::Ascii, false),
        "入力欄の ASCII は文字を入力欄に入れるので、素通しにはしない"
    );
    ed.type_keys("<C-j>Kana<CR>");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] りんぜ: あアbかな")
    );
    ed.type_keys("<CR>");
    assert_eq!(
        (ed.buffer.as_str(), ed.mode()),
        ("あアbかな", Mode::Katakana),
        "登録を終えると外側のモードに戻る"
    );
    assert_eq!(ed.table.registered, [registered("りんぜ", "あアbかな")]);
}

#[test]
fn 入れ子の入力欄もひらがなで始まり_やめると外側の入力欄のモードに戻る() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("<C-q>Mayone-zu<Space>");
    ed.type_keys("qMayo");
    assert_eq!(
        ed.registration().as_deref(),
        Some("[辞書登録] まよねーず: ▽マヨ")
    );
    ed.type_keys("<Space>");
    assert_eq!(
        (ed.registration().as_deref(), ed.mode()),
        (Some("[[辞書登録]] まよ: "), Mode::Hiragana)
    );
    ed.type_keys("<C-g>");
    assert_eq!(
        (ed.registration().as_deref(), ed.mode()),
        (Some("[辞書登録] まよねーず: ▽マヨ"), Mode::Katakana)
    );
    ed.type_keys("<C-g><C-g>");
    assert_eq!(
        (ed.preedit(), ed.registration(), ed.mode()),
        ("▽ﾏﾖﾈｰｽﾞ", None, Mode::HalfwidthKatakana)
    );
}

#[test]
fn カタカナモードで送りありの語を登録する() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("qAE");
    assert_eq!(
        (ed.preedit(), ed.registration().as_deref()),
        ("▼アエ", Some("[辞書登録] あ*え: "))
    );
    ed.type_keys("和<CR>");
    assert_eq!(ed.buffer, "和エ");
    assert_eq!(ed.table.registered, [registered("あe", "和")]);
}

#[test]
fn flush_はモードを保ち_reset_はひらがなに戻す() {
    let mut ed = Editor::new(Table::new(&[]));
    ed.type_keys("qAsi");
    let res = ed.session.flush();
    assert_eq!((res.commit.as_str(), res.mode), ("アシ", Mode::Katakana));
    ed.type_keys("l");
    let res = ed.session.flush();
    assert_eq!((res.mode, res.passthrough), (Mode::Ascii, true));
    ed.type_keys("<C-j>qKa");
    let res = ed.session.reset();
    assert_eq!(
        (res.commit.as_str(), res.mode, res.preedit.as_str()),
        ("カ", Mode::Hiragana, "")
    );
}
