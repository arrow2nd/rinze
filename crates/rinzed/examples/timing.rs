use std::time::Instant;

use rinze_dict::Dictionary;
use rinze_engine::{Okuri, Query};
use rinze_zenz::Zenz;
use rinzed::Backend;
use rinzed::user_dict::UserDictionary;

type Case = (&'static str, &'static str, Option<(char, &'static str)>);

const CASES: &[Case] = &[
    ("", "あした", None),
    ("今日は", "てんき", None),
    ("", "かんじ", None),
    ("これで", "あ", Some(('t', "っ"))),
    ("明日", "あ", Some(('e', "え"))),
    ("", "まよねーず", None),
    ("", "りんぜ", None),
    ("", "ちゃっとじーぴーてぃー", None),
    ("今日は", "てんきがいい", None),
    ("", "ぐーぐるどらいぶ", None),
];

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let runs: usize = std::env::args().nth(1).map_or(11, |s| s.parse().unwrap());
    let backend = Backend::new(
        Dictionary::from_bytes(&std::fs::read(std::env::var("RINZE_DICT").unwrap()).unwrap()),
        UserDictionary::default(),
        Zenz::load(std::env::var("RINZE_MODEL").unwrap().as_ref(), 8).unwrap(),
    );
    println!(
        "読み\t辞書の候補数\t並べ替え ms (中央値/最小/最大)\t生成 ms (中央値/最小/最大)\t生成した候補"
    );
    for &(left, reading, okuri) in CASES {
        let okuri = okuri.map(|(consonant, kana)| Okuri {
            consonant,
            kana: kana.into(),
        });
        let query = Query {
            left_context: left,
            reading,
            okuri: okuri.as_ref(),
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
            "{reading}{}",
            okuri
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
