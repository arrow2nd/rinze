//! 試作 (rust-spike) と同じ並び順になることを、cases.json の 40 件で確かめる。
//! モデルと辞書が要るので、`RINZE_MODEL` と `RINZE_DICT` が与えられたときだけ走る。

use std::sync::Mutex;

use rinze_dict::Dictionary;
use rinze_engine::{Okuri, Query};
use rinze_zenz::Zenz;
use rinzed::Backend;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    id: String,
    left: String,
    yomi: String,
    okuri_key: String,
    okuri: String,
    /// 試作が出した並び順。
    order: Vec<String>,
}

#[test]
fn 試作と同じ順位に並べる() {
    let (Ok(model), Ok(dict)) = (std::env::var("RINZE_MODEL"), std::env::var("RINZE_DICT")) else {
        eprintln!("RINZE_MODEL と RINZE_DICT が無いので飛ばす");
        return;
    };
    let backend = Backend {
        dict: Dictionary::from_bytes(&std::fs::read(dict).unwrap()),
        zenz: Mutex::new(Zenz::load(model.as_ref(), 8).unwrap()),
    };
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/spike_ranking.json")).unwrap();
    assert_eq!(cases.len(), 40);

    let mismatches: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let okuri = case.okuri_key.chars().next().map(|consonant| Okuri {
                consonant,
                kana: case.okuri.clone(),
            });
            let got = backend.candidates(&Query {
                left_context: &case.left,
                reading: &case.yomi,
                okuri: okuri.as_ref(),
            });
            (got != case.order)
                .then(|| format!("{}: 期待 {:?}\n    実際 {:?}", case.id, case.order, got))
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} 件で並び順が違う:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
