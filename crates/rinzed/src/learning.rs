//! 確定した候補への加点 (学習)。zenz の点数と履歴だけを受け取る純粋な関数にしてある。
//!
//! 見出しごとに、最後に選び直した候補 (最初に出た候補でなかったもの) だけを、zenz の点数 (対数尤度の和) に足して並べる。

/// 1 つの見出しで選び直したことのある候補。`last` は最後に選び直した時刻 (UNIX 時刻のミリ秒)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub word: String,
    pub last: u64,
}

/// 左文脈があるときの加点 (nat)。これより僅差の候補だけを入れ替える。
pub const WITH_CONTEXT: f64 = 0.75;
/// 左文脈が空のときの加点 (nat)。zenz に手がかりが無いので、履歴を強く効かせる。
pub const WITHOUT_CONTEXT: f64 = 3.0;

/// `word` への加点。見出しの履歴 `choices` のうち、最後に選び直した候補にだけ加点する。
pub fn bonus(choices: &[Choice], word: &str, left_context: &str) -> f64 {
    let latest = choices.iter().max_by_key(|c| c.last);
    if latest.is_none_or(|c| c.word != word) {
        return 0.0;
    }
    if left_context.trim().is_empty() {
        WITHOUT_CONTEXT
    } else {
        WITH_CONTEXT
    }
}

/// `scores` は `words` と同じ順の点数。点数と加点の和の大きい順に並べ、同点なら元の順を保つ。
pub fn rank(words: Vec<String>, scores: &[f64], bonus: impl Fn(&str) -> f64) -> Vec<String> {
    let total: Vec<f64> = words
        .iter()
        .zip(scores)
        .map(|(word, score)| score + bonus(word))
        .collect();
    let mut order: Vec<usize> = (0..words.len()).collect();
    order.sort_by(|&a, &b| total[b].total_cmp(&total[a]));
    order.into_iter().map(|i| words[i].clone()).collect()
}

/// 時刻が前後しても (別の rinzed の記録と合わせるときなど)、新しいほうを残す。
pub fn record(choices: &mut Vec<Choice>, word: &str, at: u64) {
    match choices.iter_mut().find(|c| c.word == word) {
        Some(choice) => choice.last = choice.last.max(at),
        None => choices.push(Choice {
            word: word.to_string(),
            last: at,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(entries: &[(&str, u64)]) -> Vec<Choice> {
        let mut choices = Vec::new();
        for (word, at) in entries {
            record(&mut choices, word, *at);
        }
        choices
    }

    fn ranked(scores: &[(&str, f64)], choices: &[Choice], left: &str) -> Vec<String> {
        let words = scores.iter().map(|(w, _)| w.to_string()).collect();
        let scores: Vec<f64> = scores.iter().map(|(_, s)| *s).collect();
        rank(words, &scores, |w| bonus(choices, w, left))
    }

    #[test]
    fn 履歴が無ければ点数の順に並べる() {
        assert_eq!(
            ranked(
                &[("会", -3.0), ("和", -1.0), ("合", -2.0)],
                &[],
                "マヨネーズを"
            ),
            ["和", "合", "会"]
        );
    }

    #[test]
    fn 僅差なら選び直した候補が上がり_点差が大きければ文脈が勝つ() {
        let chose = history(&[("浩", 1)]);
        assert_eq!(
            ranked(&[("博", -5.0), ("浩", -5.5)], &chose, "友達の")[0],
            "浩"
        );
        assert_eq!(
            ranked(&[("博", -5.0), ("浩", -6.0)], &chose, "友達の")[0],
            "博",
            "文脈があるときの加点は 1 nat に届かない"
        );
        assert_eq!(
            ranked(&[("博", -5.0), ("浩", -7.5)], &chose, "")[0],
            "浩",
            "左文脈が無ければ加点を大きくする"
        );
        assert_eq!(
            ranked(&[("博", -5.0), ("浩", -8.5)], &chose, " ")[0],
            "博",
            "空白だけの左文脈も文脈なしとして扱い、それでも点差が大きければ zenz の順"
        );
    }

    #[test]
    fn 最後に選び直した候補だけに加点する() {
        let chose = history(&[("浩", 1), ("博", 2)]);
        assert_eq!(bonus(&chose, "博", "友達の"), WITH_CONTEXT);
        assert_eq!(bonus(&chose, "浩", "友達の"), 0.0);
        let chose_again = history(&[("浩", 1), ("博", 2), ("浩", 3)]);
        assert_eq!(bonus(&chose_again, "浩", "友達の"), WITH_CONTEXT);
        assert_eq!(bonus(&chose_again, "博", "友達の"), 0.0);
    }

    #[test]
    fn 記録は時刻の新しいほうを残す() {
        let mut choices = history(&[("浩", 5)]);
        record(&mut choices, "浩", 3);
        assert_eq!(
            choices,
            [Choice {
                word: "浩".into(),
                last: 5
            }]
        );
    }

    #[test]
    fn 同点なら元の順を保つ() {
        let none = ranked(&[("ファイル", 0.0), ("フィール", 0.0)], &[], "");
        assert_eq!(none, ["ファイル", "フィール"]);
        let chose = history(&[("フィール", 1)]);
        let chosen_first = ranked(&[("ファイル", 0.0), ("フィール", 0.0)], &chose, "");
        assert_eq!(chosen_first, ["フィール", "ファイル"]);
    }
}
