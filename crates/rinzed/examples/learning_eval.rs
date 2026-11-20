//! 選んだ候補への加点 (学習) の形と大きさを決めるための測定。
//!
//! zenz の点数は左文脈と候補だけで決まり、履歴では変わらない。そこで各場面の点数を 1 度だけ計算し、
//! 記録する確定、加点の形と大きさを変えながら、履歴を積んだあとの 1 位を数える。
//!
//! - 引っぱられない: 同じ見出しで文脈により答えが変わる組 (A, B) のうち、学習なしで B の文脈の 1 位が B の答えだったものについて、
//!   A の文脈で A の答えを N 回確定したあとも B の答えが 1 位である割合。
//!   「最悪」は、記録のしかたによらず A の答えの確定を毎回記録した場合 (A の答えを毎回送って選んだ場合に当たる)。
//! - 覚える: 学習なしで 1 位が答えでなかった場面について、同じ文脈で答えを k 回確定したあと答えが 1 位になる割合。
//!   「戻す」は、答えを 1 回確定してから学習なしの 1 位を 1 回確定したあと、学習なしの 1 位が 1 位に戻る割合。
//!   「再び」は、そのあとさらに答えを 1 回確定したあと、答えが 1 位になる割合。
//! - コーパス: `crates/rinze-engine/tests/fixtures/corpus.tsv` を履歴の空の状態から 2 周打ち、各区間で期待する表記を確定する。
//!   1 周目 (学習しながら) と 2 周目の 1 位正解率を出す。`PASSES=2 tests/nvim/corpus.sh` の結果と区間ごとに一致する。
//!
//! 環境変数 `DETAIL=<形>:<大きさ>` (`latest:0.75`、rinzed の実装は `library:-` など) を与えると、送ったときだけ覚える設定で、
//! 引っぱられた組、覚えられなかった場面、コーパスの 2 周目で 1 位が変わった区間を出す。`DETAIL_WITHOUT_CONTEXT` で左文脈が空のときの大きさを変える。
//!
//! `RINZE_MODEL=... RINZE_DICT=... cargo run --release -p rinzed --example learning_eval`

use std::collections::HashMap;

use rinze_engine::{Converter, Key, Midashi, Okuri, Pick, Query, Session};
use rinze_zenz::{Model, Zenz};
use rinzed::learning::{self, Choice};
use rinzed::user_dict::UserDictionary;
use rinzed::{Backend, Learning, lookup};
use serde::Deserialize;

const PULL_COMMITS: [usize; 2] = [1, 20];
const LEARN_COMMITS: [usize; 2] = [1, 3];
const MAGNITUDES: [f64; 10] = [0.25, 0.5, 0.6, 0.75, 0.9, 1.0, 1.25, 1.5, 2.0, 3.0];
const WITHOUT_CONTEXT: [f64; 6] = [1.5, 2.0, 3.0, 4.0, 6.0, 100.0];

#[derive(Deserialize)]
struct Case {
    group: String,
    id: String,
    left: String,
    yomi: String,
    okuri_key: String,
    okuri: String,
    answer: Vec<String>,
}

/// 1 つの変換の場面。`scores` は `words` と同じ順で、abbrev では無い。
struct Scene {
    id: String,
    left: String,
    key: String,
    words: Vec<String>,
    scores: Option<Vec<f64>>,
    generated_first: Option<String>,
    answer: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Fixed,
    Saturating,
    HalvedByRecency,
    LatestOnly,
    Library,
}

const SHAPES: [Shape; 5] = [
    Shape::Fixed,
    Shape::Saturating,
    Shape::HalvedByRecency,
    Shape::LatestOnly,
    Shape::Library,
];

#[derive(Clone, Copy, PartialEq)]
enum Policy {
    EveryCommit,
    OnlyAfterSending,
}

#[derive(Clone, Copy)]
struct Config {
    shape: Shape,
    magnitude: f64,
    /// 左文脈が空のときの大きさ。無ければ `magnitude`。
    without_context: Option<f64>,
    policy: Policy,
}

/// 回数で飽和する形のために、rinzed の履歴 ([`Choice`]) に無い回数も数える。
struct Seen {
    word: String,
    count: u32,
    last: u64,
}

#[derive(Default)]
struct History {
    clock: u64,
    choices: HashMap<String, Vec<Seen>>,
}

impl History {
    fn record(&mut self, key: &str, word: &str) {
        self.clock += 1;
        let seen = self.choices.entry(key.to_string()).or_default();
        match seen.iter_mut().find(|s| s.word == word) {
            Some(s) => {
                s.count += 1;
                s.last = self.clock;
            }
            None => seen.push(Seen {
                word: word.to_string(),
                count: 1,
                last: self.clock,
            }),
        }
    }
}

impl Shape {
    fn name(self) -> &'static str {
        match self {
            Shape::Fixed => "固定",
            Shape::Saturating => "回数で飽和",
            Shape::HalvedByRecency => "新しさで半減",
            Shape::LatestOnly => "最新だけ",
            Shape::Library => "rinzed の実装",
        }
    }

    fn parse(name: &str) -> Shape {
        match name {
            "fixed" => Shape::Fixed,
            "saturating" => Shape::Saturating,
            "halved" => Shape::HalvedByRecency,
            "latest" => Shape::LatestOnly,
            "library" => Shape::Library,
            _ => panic!("未知の形: {name}"),
        }
    }

    fn bonus(self, magnitude: f64, choices: &[Seen], word: &str, left_context: &str) -> f64 {
        if self == Shape::Library {
            let choices: Vec<Choice> = choices
                .iter()
                .map(|s| Choice {
                    word: s.word.clone(),
                    last: s.last,
                })
                .collect();
            return learning::bonus(&choices, word, left_context);
        }
        let Some(choice) = choices.iter().find(|c| c.word == word) else {
            return 0.0;
        };
        let newer = choices.iter().filter(|c| c.last > choice.last).count();
        match self {
            Shape::Fixed => magnitude,
            Shape::Saturating => magnitude * (1.0 - 0.5f64.powi(choice.count as i32)),
            Shape::HalvedByRecency => magnitude * 0.5f64.powi(newer as i32),
            Shape::LatestOnly if newer == 0 => magnitude,
            Shape::LatestOnly => 0.0,
            Shape::Library => unreachable!(),
        }
    }
}

impl Scene {
    fn top(&self, history: &History, config: Config) -> Option<String> {
        if self.words.is_empty() {
            return self.generated_first.clone();
        }
        let choices = history
            .choices
            .get(&self.key)
            .map_or(&[][..], Vec::as_slice);
        let magnitude = match config.without_context {
            Some(m) if self.left.trim().is_empty() => m,
            _ => config.magnitude,
        };
        let bonus = |word: &str| config.shape.bonus(magnitude, choices, word, &self.left);
        let zeros = vec![0.0; self.words.len()];
        let scores = self.scores.as_ref().unwrap_or(&zeros);
        learning::rank(self.words.clone(), scores, bonus)
            .into_iter()
            .next()
    }

    fn correct(&self, history: &History, config: Config) -> bool {
        self.top(history, config)
            .is_some_and(|top| self.answer.contains(&top))
    }

    fn commit(&self, word: &str, history: &mut History, config: Config) {
        let sent = self.top(history, config).as_deref() != Some(word);
        if config.policy == Policy::EveryCommit || sent {
            history.record(&self.key, word);
        }
    }

    fn commit_answer(&self, times: usize, history: &mut History, config: Config) {
        for _ in 0..times {
            self.commit(&self.answer[0], history, config);
        }
    }

    fn baseline_top(&self) -> String {
        self.top(&History::default(), BASELINE).unwrap()
    }
}

const BASELINE: Config = Config {
    shape: Shape::Fixed,
    magnitude: 0.0,
    without_context: None,
    policy: Policy::EveryCommit,
};

struct Rate(usize, usize);

impl std::fmt::Display for Rate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.0, self.1)
    }
}

fn rate<T>(items: &[T], ok: impl Fn(&T) -> bool) -> Rate {
    Rate(items.iter().filter(|i| ok(i)).count(), items.len())
}

fn pull_pairs(scenes: &[Scene]) -> Vec<(&Scene, &Scene)> {
    let mut pairs = Vec::new();
    for a in scenes {
        for b in scenes {
            let disjoint = !a.answer.iter().any(|w| b.answer.contains(w));
            if a.key == b.key && disjoint && b.correct(&History::default(), BASELINE) {
                pairs.push((a, b));
            }
        }
    }
    pairs
}

fn held(a: &Scene, b: &Scene, times: usize, config: Config) -> bool {
    let mut history = History::default();
    a.commit_answer(times, &mut history, config);
    b.correct(&history, config)
}

fn learned(s: &Scene, times: usize, config: Config) -> bool {
    let mut history = History::default();
    s.commit_answer(times, &mut history, config);
    s.correct(&history, config)
}

/// 答えを覚えさせてから学習なしの 1 位に戻し、もう一度答えを選ぶ。戻したときと再び選んだときに、それぞれ 1 位が選んだものか。
fn switched(s: &Scene, config: Config) -> (bool, bool) {
    let mut history = History::default();
    let before = s.baseline_top();
    s.commit_answer(1, &mut history, config);
    s.commit(&before, &mut history, config);
    let back = s.top(&history, config).as_ref() == Some(&before);
    s.commit_answer(1, &mut history, config);
    (back, s.correct(&history, config))
}

fn corpus_passes(sections: &[Scene], config: Config) -> [Rate; 2] {
    let mut history = History::default();
    [(); 2].map(|()| {
        let mut hits = 0;
        for s in sections {
            hits += usize::from(s.correct(&history, config));
            s.commit_answer(1, &mut history, config);
        }
        Rate(hits, sections.len())
    })
}

struct Oracle<'a, 'm> {
    backend: &'a Backend<'m>,
    expect: Option<String>,
    scenes: Vec<Scene>,
}

impl Oracle<'_, '_> {
    fn scene(&self, query: &Query, expect: &str) -> Scene {
        let words = lookup(&self.backend.dict, &self.backend.user(), query.midashi);
        let scores = match query.midashi {
            Midashi::Kana { .. } if words.len() >= 2 => Some(
                self.backend
                    .score(query.left_context, query.midashi, &words)
                    .unwrap(),
            ),
            Midashi::Kana { .. } => Some(vec![0.0; words.len()]),
            Midashi::Abbrev(_) => None,
        };
        let generated_first = words
            .is_empty()
            .then(|| self.backend.generate(query).into_iter().next())
            .flatten();
        Scene {
            id: format!("corpus {}", self.scenes.len() + 1),
            left: query.left_context.to_string(),
            key: query.midashi.key(),
            words,
            scores,
            generated_first,
            answer: vec![expect.to_string()],
        }
    }
}

impl Converter for Oracle<'_, '_> {
    fn candidates(&mut self, query: &Query) -> Vec<String> {
        let okuri = query.midashi.okuri().map_or("", |o| o.kana.as_str());
        let expect = self.expect.take().expect("区間の外で変換が始まった");
        let expect = expect.strip_suffix(okuri).unwrap_or(&expect).to_string();
        let scene = self.scene(query, &expect);
        self.scenes.push(scene);
        vec![expect]
    }

    fn generate(&mut self, _: &Query) -> Vec<String> {
        Vec::new()
    }

    fn register(&mut self, _: &Midashi, _: &str) {}

    fn purge(&mut self, _: &Midashi, _: &str) {}

    fn committed(&mut self, _: &Midashi, _: &str, _: Pick) {}
}

fn keys(text: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let special = [
            ("<Space>", Key::Space),
            ("<CR>", Key::Enter),
            ("<BS>", Key::Backspace),
            ("<C-g>", Key::Cancel),
            ("<C-j>", Key::Kakutei),
            ("<C-q>", Key::ToggleHalfwidth),
        ]
        .into_iter()
        .find(|(name, _)| rest.starts_with(name));
        let (key, len) = special.map_or((Key::Char(c), c.len_utf8()), |(name, key)| {
            (key, name.len())
        });
        keys.push(key);
        rest = &rest[len..];
    }
    keys
}

fn corpus_scenes(backend: &Backend<'_>) -> Vec<Scene> {
    let mut oracle = Oracle {
        backend,
        expect: None,
        scenes: Vec::new(),
    };
    for line in include_str!("../../rinze-engine/tests/fixtures/corpus.tsv")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (_, mut rest) = line.split_once('\t').unwrap();
        let mut session = Session::default();
        let mut buffer = String::new();
        loop {
            let (before, expect, after) = match rest.split_once('{') {
                Some((before, after)) => {
                    let (expect, after) = after.split_once('}').unwrap();
                    (before, Some(expect), after)
                }
                None => (rest, None, ""),
            };
            oracle.expect = expect.map(str::to_string);
            for key in keys(before) {
                let res = session.handle(key, &buffer, &mut oracle);
                buffer.push_str(&res.commit);
                if !res.handled {
                    match key {
                        Key::Char(c) => buffer.push(c),
                        Key::Space => buffer.push(' '),
                        _ => {}
                    }
                }
            }
            if expect.is_none() {
                break;
            }
            rest = after;
        }
    }
    oracle.scenes
}

fn case_scene(backend: &Backend<'_>, case: &Case) -> Scene {
    let midashi = Midashi::Kana {
        reading: case.yomi.clone(),
        okuri: case.okuri_key.chars().next().map(|consonant| Okuri {
            consonant,
            kana: case.okuri.clone(),
        }),
    };
    let words = lookup(&backend.dict, &backend.user(), &midashi);
    let scores = backend.score(&case.left, &midashi, &words).unwrap();
    Scene {
        id: format!("{} {}", case.group, case.id),
        left: case.left.clone(),
        key: midashi.key(),
        words,
        scores: Some(scores),
        generated_first: None,
        answer: case.answer.clone(),
    }
}

/// 答えの点数が最も良い候補からどれだけ低いか。答えが 1 位なら、2 位との差を負で返す。
fn gap(scene: &Scene, word_against: Option<&str>) -> f64 {
    let scores = scene.scores.as_ref().unwrap();
    let score = |w: &str| scene.words.iter().position(|x| x == w).map(|i| scores[i]);
    let answer = scene
        .answer
        .iter()
        .filter_map(|w| score(w))
        .fold(f64::NEG_INFINITY, f64::max);
    let against = match word_against {
        Some(w) => score(w).unwrap(),
        None => scene
            .words
            .iter()
            .zip(scores)
            .filter(|(w, _)| !scene.answer.contains(w))
            .map(|(_, s)| *s)
            .fold(f64::NEG_INFINITY, f64::max),
    };
    against - answer
}

fn print_gaps(pairs: &[(&Scene, &Scene)], learn: &[&Scene]) {
    println!(
        "## 引っぱられうる組 ({} 組): B の文脈での B の答えと A の答えの点差 (nat)",
        pairs.len()
    );
    let mut pull: Vec<(f64, String)> = pairs
        .iter()
        .map(|(a, b)| {
            let needs_sending = !a.correct(&History::default(), BASELINE);
            (
                -gap(b, Some(&a.answer[0])),
                format!(
                    "[{}] {} > {}{}",
                    b.left,
                    b.answer[0],
                    a.answer[0],
                    if needs_sending {
                        " (A の文脈で A の答えは 1 位でない)"
                    } else {
                        ""
                    }
                ),
            )
        })
        .collect();
    pull.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (g, label) in &pull {
        println!("{g:6.2}\t{label}");
    }
    println!(
        "\n## 覚える場面 ({} 件): 1 位と答えの点差 (nat)",
        learn.len()
    );
    let mut gaps: Vec<(f64, String)> = learn
        .iter()
        .map(|s| {
            (
                gap(s, None),
                format!("{} [{}] {} → {}", s.id, s.left, s.key, s.answer[0]),
            )
        })
        .collect();
    gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (g, label) in &gaps {
        println!("{g:6.2}\t{label}");
    }
}

fn print_detail(config: Config, pairs: &[(&Scene, &Scene)], learn: &[&Scene], corpus: &[Scene]) {
    let mut history = History::default();
    for s in corpus {
        s.commit_answer(1, &mut history, config);
    }
    for s in corpus {
        let before = s.correct(&History::default(), BASELINE);
        let after = s.correct(&history, config);
        if before != after {
            println!(
                "コーパス 2 周目で{}: {} [{}] {} → {} (1 位 {:?})",
                if after { "当たった" } else { "外れた" },
                s.id,
                s.left,
                s.key,
                s.answer[0],
                s.top(&history, config)
            );
        }
        s.commit_answer(1, &mut history, config);
    }
    println!(
        "\n## {}:{} で外れたもの",
        config.shape.name(),
        config.magnitude
    );
    for (a, b) in pairs {
        if !held(a, b, PULL_COMMITS[1], config) {
            println!(
                "引っぱられた: [{}] {} のあと [{}] {}",
                a.left, a.answer[0], b.left, b.answer[0]
            );
        }
        let worst = Config {
            policy: Policy::EveryCommit,
            ..config
        };
        if !held(a, b, PULL_COMMITS[1], worst) {
            println!(
                "最悪で引っぱられた: [{}] {} のあと [{}] {}",
                a.left, a.answer[0], b.left, b.answer[0]
            );
        }
    }
    for s in learn {
        if !learned(s, LEARN_COMMITS[1], config) {
            println!(
                "覚えなかった: {} [{}] {} → {}",
                s.id, s.left, s.key, s.answer[0]
            );
        }
    }
}

fn main() {
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
    let cases: Vec<Case> = serde_json::from_str(include_str!("learning_cases.json")).unwrap();
    let scenes: Vec<Scene> = cases.iter().map(|c| case_scene(&backend, c)).collect();
    let corpus = corpus_scenes(&backend);

    let baseline = History::default();
    let pairs = pull_pairs(&scenes);
    let learn: Vec<&Scene> = scenes
        .iter()
        .chain(&corpus)
        .filter(|s| !s.correct(&baseline, BASELINE) && s.scores.is_some())
        .filter(|s| s.words.iter().any(|w| s.answer.contains(w)))
        .collect();
    print_gaps(&pairs, &learn);
    for s in corpus.iter().filter(|s| {
        !s.correct(&baseline, BASELINE) && !s.words.iter().any(|w| s.answer.contains(w))
    }) {
        println!(
            "(辞書に無いので対象外) {} [{}] {} → {}",
            s.id, s.left, s.key, s.answer[0]
        );
    }

    let [c1, c2] = corpus_passes(&corpus, BASELINE);
    println!("\n学習なし: コーパス {c1} / {c2}\n");
    println!(
        "| 記録 | 形 | 大きさ | 引っぱられない 1 回 | 20 回 | 最悪 20 回 | 覚える 1 回 | 3 回 | 戻す | 再び | コーパス 1 周目 | 2 周目 |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    let rows = [Policy::EveryCommit, Policy::OnlyAfterSending]
        .into_iter()
        .flat_map(|policy| SHAPES.into_iter().map(move |shape| (policy, shape)))
        .flat_map(|(policy, shape)| {
            let magnitudes: &[f64] = if shape == Shape::Library {
                &[f64::NAN]
            } else {
                &MAGNITUDES
            };
            magnitudes.iter().map(move |&magnitude| Config {
                shape,
                magnitude,
                without_context: None,
                policy,
            })
        });
    let rows = rows.chain([0.75, 0.9].into_iter().flat_map(|magnitude| {
        WITHOUT_CONTEXT.into_iter().map(move |m| Config {
            shape: Shape::LatestOnly,
            magnitude,
            without_context: Some(m),
            policy: Policy::OnlyAfterSending,
        })
    }));
    for config in rows {
        let worst = Config {
            policy: Policy::EveryCommit,
            ..config
        };
        let [c1, c2] = corpus_passes(&corpus, config);
        let switches: Vec<(bool, bool)> = learn.iter().map(|s| switched(s, config)).collect();
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {c1} | {c2} |",
            match config.policy {
                Policy::EveryCommit => "すべて",
                Policy::OnlyAfterSending => "送ったときだけ",
            },
            config.shape.name(),
            match (config.magnitude.is_nan(), config.without_context) {
                (true, _) => "-".to_string(),
                (false, None) => config.magnitude.to_string(),
                (false, Some(m)) => format!("{} (文脈なし {m})", config.magnitude),
            },
            rate(&pairs, |(a, b)| held(a, b, PULL_COMMITS[0], config)),
            rate(&pairs, |(a, b)| held(a, b, PULL_COMMITS[1], config)),
            rate(&pairs, |(a, b)| held(a, b, PULL_COMMITS[1], worst)),
            rate(&learn, |s| learned(s, LEARN_COMMITS[0], config)),
            rate(&learn, |s| learned(s, LEARN_COMMITS[1], config)),
            rate(&switches, |s| s.0),
            rate(&switches, |s| s.1),
        );
    }
    if let Ok(detail) = std::env::var("DETAIL") {
        let (shape, magnitude) = detail.split_once(':').unwrap();
        let config = Config {
            shape: Shape::parse(shape),
            magnitude: magnitude.parse().unwrap_or(f64::NAN),
            without_context: std::env::var("DETAIL_WITHOUT_CONTEXT")
                .ok()
                .map(|m| m.parse().unwrap()),
            policy: Policy::OnlyAfterSending,
        };
        print_detail(config, &pairs, &learn, &corpus);
    }
}
