use rinze_dict::Dictionary;
use rinze_engine::{Midashi, Okuri, Query};
use rinze_zenz::{Model, Zenz};
use rinzed::user_dict::UserDictionary;
use rinzed::{Backend, Learning};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    id: String,
    left: String,
    yomi: String,
    okuri_key: String,
    okuri: String,
    #[serde(rename = "order")]
    spike_order: Vec<String>,
}

#[test]
fn 試作と同じ順位に並べる() {
    let (Ok(model), Ok(dict)) = (std::env::var("RINZE_MODEL"), std::env::var("RINZE_DICT")) else {
        eprintln!("RINZE_MODEL と RINZE_DICT が無いので飛ばす");
        return;
    };
    let model = Model::load(model.as_ref()).unwrap();
    let backend = Backend::new(
        Dictionary::from_bytes(&std::fs::read(dict).unwrap()),
        UserDictionary::in_memory(),
        Zenz::new(&model, 8).unwrap(),
        Learning::Off,
    );
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
            let midashi = Midashi::Kana {
                reading: case.yomi.clone(),
                okuri,
            };
            let got = backend.candidates(&Query {
                left_context: &case.left,
                midashi: &midashi,
            });
            (got != case.spike_order).then(|| {
                format!(
                    "{}: 期待 {:?}\n    実際 {:?}",
                    case.id, case.spike_order, got
                )
            })
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} 件で並び順が違う:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
