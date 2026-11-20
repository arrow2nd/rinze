use std::time::Instant;

use rinze_engine::{Midashi, Okuri, Query};
use rinze_zenz::{Model, Zenz};
use rinzed::user_dict::UserDictionary;
use rinzed::{Backend, Learning};

struct Case {
    left_context: &'static str,
    reading: &'static str,
    okuri: Option<Okuri>,
}

fn case(left_context: &'static str, reading: &'static str) -> Case {
    Case {
        left_context,
        reading,
        okuri: None,
    }
}

fn okuri_case(
    left_context: &'static str,
    reading: &'static str,
    consonant: char,
    kana: &str,
) -> Case {
    Case {
        left_context,
        reading,
        okuri: Some(Okuri {
            consonant,
            kana: kana.into(),
        }),
    }
}

fn cases() -> Vec<Case> {
    vec![
        case("", "あした"),
        case("今日は", "てんき"),
        case("", "かんじ"),
        okuri_case("これで", "あ", 't', "っ"),
        okuri_case("明日", "あ", 'e', "え"),
        case("", "まよねーず"),
        case("", "りんぜ"),
        case("", "ちゃっとじーぴーてぃー"),
        case("今日は", "てんきがいい"),
        case("", "ぐーぐるどらいぶ"),
    ]
}
fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let runs: usize = std::env::args().nth(1).map_or(11, |s| s.parse().unwrap());
    let model = Model::load(std::env::var("RINZE_MODEL").unwrap().as_ref()).unwrap();
    let backend = Backend::new(
        rinzed::load_dictionaries(std::env::split_paths(
            &std::env::var_os("RINZE_DICT").unwrap(),
        ))
        .unwrap(),
        UserDictionary::in_memory(),
        Zenz::new(&model, 8).unwrap(),
        Learning::Off,
    );
    println!(
        "読み\t辞書の候補数\t並べ替え ms (中央値/最小/最大)\t生成 ms (中央値/最小/最大)\t生成した候補"
    );
    for case in cases() {
        let midashi = Midashi::Kana {
            reading: case.reading.to_string(),
            okuri: case.okuri.clone(),
        };
        let query = Query {
            left_context: case.left_context,
            midashi: &midashi,
        };
        let (mut rank, mut generate) = (Vec::new(), Vec::new());
        let (mut n, mut words) = (0, Vec::new());
        for _ in 0..runs {
            let t = Instant::now();
            n = backend.candidates(&query).len();
            rank.push(t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            words = backend.generate(&query);
            generate.push(t.elapsed().as_secs_f64() * 1e3);
        }
        let stats = |v: &Vec<f64>| {
            format!(
                "{:.1}/{:.1}/{:.1}",
                median(v.clone()),
                v.iter().copied().fold(f64::INFINITY, f64::min),
                v.iter().copied().fold(0.0, f64::max)
            )
        };
        let shown = format!(
            "{}{}",
            case.reading,
            case.okuri
                .as_ref()
                .map_or(String::new(), |o| format!("*{}", o.kana))
        );
        println!(
            "{shown}\t{n}\t{}\t{}\t{words:?}",
            stats(&rank),
            stats(&generate)
        );
    }
}
