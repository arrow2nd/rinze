//! abbrev の候補を zenz で並べ替えるかを決めるための測定。
//! 英字の見出しの候補を、辞書の順と、英字をそのまま読みとして zenz に渡した点数の順で並べ、期待する表記の順位を比べる。
//!
//! `RINZE_MODEL=... RINZE_DICT=... cargo run --release -p rinzed --example abbrev_ranking`

use rinze_dict::Dictionary;
use rinze_zenz::{Model, Zenz};

const CASES: &[(&str, &str, &str)] = &[
    ("この", "server", "サーバー"),
    ("入力した", "data", "データ"),
    ("ログインした", "user", "ユーザー"),
    ("", "memory", "メモリ"),
    ("ビルドで", "error", "エラー"),
    ("完成した", "image", "イメージ"),
    ("画面に", "text", "テキスト"),
    ("次の", "page", "ページ"),
    ("", "menu", "メニュー"),
    ("OSの", "update", "アップデート"),
    ("", "window", "ウィンドウ"),
    ("友達に", "mail", "メール"),
    ("明日の", "test", "テスト"),
    ("新しい", "version", "バージョン"),
    ("", "tag", "タグ"),
    ("コードの", "review", "レビュー"),
    ("", "config", "コンフィグ"),
    ("データベースの", "table", "テーブル"),
    ("", "directory", "ディレクトリ"),
    ("", "network", "ネットワーク"),
    ("", "software", "ソフトウェア"),
    ("", "hardware", "ハードウェア"),
    ("", "program", "プログラム"),
    ("", "center", "センター"),
    ("", "service", "サービス"),
    ("毎週の", "meeting", "ミーティング"),
    ("", "report", "レポート"),
    ("", "document", "ドキュメント"),
    ("", "demo", "デモ"),
    ("", "linux", "Linux"),
    ("期末", "test", "試験"),
    ("結果を", "table", "表"),
    ("定例の", "meeting", "会議"),
];

fn rank(words: &[String], expected: &str) -> Option<usize> {
    words.iter().position(|w| w == expected)
}

fn main() {
    let model = Model::load(std::env::var("RINZE_MODEL").unwrap().as_ref()).unwrap();
    let dict =
        Dictionary::from_bytes(&std::fs::read(std::env::var("RINZE_DICT").unwrap()).unwrap());
    let mut zenz = Zenz::new(&model, 8).unwrap();
    let (mut dict_top, mut zenz_top) = (0, 0);
    println!("左文脈\t見出し\t期待\t辞書の順位\tzenz の順位\tzenz の順");
    for &(left, key, expected) in CASES {
        let words: Vec<String> = dict.lookup(key).map(String::from).collect();
        let scores = zenz.score(left, key, &words).unwrap();
        let mut order: Vec<usize> = (0..words.len()).collect();
        order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
        let by_zenz: Vec<String> = order.into_iter().map(|i| words[i].clone()).collect();
        let (d, z) = (rank(&words, expected), rank(&by_zenz, expected));
        dict_top += usize::from(d == Some(0));
        zenz_top += usize::from(z == Some(0));
        println!("{left}\t{key}\t{expected}\t{d:?}\t{z:?}\t{by_zenz:?}");
    }
    println!(
        "1 位: 辞書の順 {dict_top}/{n}、zenz {zenz_top}/{n}",
        n = CASES.len()
    );
}
