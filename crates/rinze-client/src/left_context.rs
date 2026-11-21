/// 入力先のアプリが知らせた、カーソルの前後の文字列。`cursor` は文字数で数える。
#[derive(Debug, Clone, Copy)]
pub struct Surrounding<'a> {
    pub text: &'a str,
    pub cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Surrounding,
    /// 入力先の文字列に rinze の未確定文字列が入っていたので、それを除いた。
    PreeditRemoved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftContext {
    pub text: String,
    pub origin: Origin,
}

// 1 つの入力欄に表示した未確定文字列の跡。surrounding text から rinze の未確定文字列を除くのに使う。
//
// Firefox は未確定文字列を本文に入れた surrounding text を送り、しかも本文が変わるまで同じものを送り続ける
// (`▼雨` を確定して次の見出しを 5 打鍵打つ間も `…▼雨` が届いた)。
// 届いた surrounding text が前と同じなら前と同じ左文脈を使い、変わっていれば、前に変わってから表示した未確定文字列と突き合わせる。
#[derive(Debug, Default)]
pub(crate) struct PreeditTrail {
    shown_since_snapshot: Vec<String>,
    displayed: String,
    last: Option<(String, usize, LeftContext)>,
}

const MAX_TRAIL: usize = 64;
/// rinzed (zenz) が使う左文脈の長さ。それより前の文字は送らない。
const MAX_LEFT_CONTEXT_CHARS: usize = 40;

impl PreeditTrail {
    pub(crate) fn displayed(&mut self, preedit: &str) {
        self.displayed = preedit.to_string();
        if preedit.is_empty()
            || self.shown_since_snapshot.last().map(String::as_str) == Some(preedit)
        {
            return;
        }
        if self.shown_since_snapshot.len() == MAX_TRAIL {
            self.shown_since_snapshot.remove(0);
        }
        self.shown_since_snapshot.push(preedit.to_string());
    }

    pub(crate) fn left_context(&mut self, surrounding: Surrounding) -> LeftContext {
        if let Some((text, cursor, left)) = &self.last
            && text == surrounding.text
            && *cursor == surrounding.cursor
        {
            return left.clone();
        }
        let recent: Vec<String> = self.shown_since_snapshot.iter().rev().cloned().collect();
        let left = left_context(surrounding, &recent);
        self.last = Some((
            surrounding.text.to_string(),
            surrounding.cursor,
            left.clone(),
        ));
        self.shown_since_snapshot.clear();
        if !self.displayed.is_empty() {
            self.shown_since_snapshot.push(self.displayed.clone());
        }
        left
    }
}

/// カーソルより前の文字列を左文脈にする。
///
/// `recent_preedits` のどれかが、カーソルをまたぐ位置にそのまま入っていれば、その始まりより前を左文脈にする。
/// Firefox はカーソルを未確定文字列の途中に置いて送ることがある (`▼明日` の表示中に、カーソルが `▼明` のあと)。
/// 未確定文字列の全体が一致することを求めるので、本文に書かれた ▽ や ▼ は除かない。
pub fn left_context(surrounding: Surrounding, recent_preedits: &[String]) -> LeftContext {
    let chars: Vec<char> = surrounding.text.chars().collect();
    let cursor = surrounding.cursor.min(chars.len());
    let preedit_start = recent_preedits.iter().find_map(|preedit| {
        let preedit: Vec<char> = preedit.chars().collect();
        if preedit.is_empty() {
            return None;
        }
        (cursor.saturating_sub(preedit.len())..cursor)
            .rev()
            .find(|&start| chars.get(start..start + preedit.len()) == Some(&preedit[..]))
    });
    let (end, origin) = match preedit_start {
        Some(start) => (start, Origin::PreeditRemoved),
        None => (cursor, Origin::Surrounding),
    };
    let begin = end.saturating_sub(MAX_LEFT_CONTEXT_CHARS);
    LeftContext {
        text: chars[begin..end].iter().collect(),
        origin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left(text: &str, cursor: usize, preedits: &[&str]) -> (String, Origin) {
        let preedits: Vec<String> = preedits.iter().map(|p| p.to_string()).collect();
        let got = left_context(Surrounding { text, cursor }, &preedits);
        (got.text, got.origin)
    }

    #[test]
    fn 未確定文字列を含まなければカーソルより前をそのまま使う() {
        assert_eq!(
            left("明日は雨", 3, &["▽あめ"]),
            ("明日は".into(), Origin::Surrounding)
        );
        assert_eq!(left("", 0, &[]), ("".into(), Origin::Surrounding));
    }

    #[test]
    fn カーソルの前に入った未確定文字列を除く() {
        assert_eq!(
            left("明日h", 3, &["▽あ", "h"]),
            ("明日".into(), Origin::PreeditRemoved)
        );
        assert_eq!(
            left("絶好の▽きかい", 7, &["▽きかい"]),
            ("絶好の".into(), Origin::PreeditRemoved)
        );
    }

    #[test]
    fn カーソルが未確定文字列の途中にあっても始まりより前を使う() {
        assert_eq!(
            left("▼明日", 2, &["h", "▼明日"]),
            ("".into(), Origin::PreeditRemoved)
        );
        assert_eq!(
            left("明日は▼雨。", 4, &["▼雨"]),
            ("明日は".into(), Origin::PreeditRemoved)
        );
    }

    #[test]
    fn 本文に書いた記号は未確定文字列の一部と一致しても除かない() {
        assert_eq!(
            left("記号▽", 3, &["▽あ"]),
            ("記号▽".into(), Origin::Surrounding)
        );
        assert_eq!(
            left("▼印のあと", 2, &["▼明日"]),
            ("▼印".into(), Origin::Surrounding)
        );
        assert_eq!(
            left("表示は▽あと書く", 4, &["▽あし"]),
            ("表示は▽".into(), Origin::Surrounding)
        );
    }

    #[test]
    fn カーソルより後ろにだけある未確定文字列は除かない() {
        assert_eq!(
            left("明日▽あ", 2, &["▽あ"]),
            ("明日".into(), Origin::Surrounding)
        );
    }

    #[test]
    fn 本文が変わるまで同じ_surrounding_text_が届いても未確定文字列を除き続ける() {
        // 入れ子の sway で Firefox に打ったときに届いた surrounding text の並び
        let mut trail = PreeditTrail::default();
        let key = |trail: &mut PreeditTrail, text: &str, cursor: usize, shown: &str| {
            let left = trail.left_context(Surrounding { text, cursor });
            trail.displayed(shown);
            (left.text, left.origin)
        };
        key(&mut trail, "明日は校明日カ", 7, "▽あ");
        key(&mut trail, "明日は校明日カ", 7, "▽あm");
        key(&mut trail, "明日は校明日カめ", 7, "▽あめ");
        let at_space = key(&mut trail, "明日は校明日カめ", 7, "▼雨");
        assert_eq!(at_space, ("明日は校明日カ".into(), Origin::Surrounding));
        let at_enter = key(&mut trail, "明日は校明日カ▼雨", 8, "");
        assert_eq!(at_enter, ("明日は校明日カ".into(), Origin::PreeditRemoved));
        for shown in ["▽k", "▽き", "▽きk", "▽きか", "▽きかい"] {
            let got = key(&mut trail, "明日は校明日カ▼雨", 8, shown);
            assert_eq!(
                got,
                ("明日は校明日カ".into(), Origin::PreeditRemoved),
                "{shown}"
            );
        }
        let fresh = key(&mut trail, "明日は校明日カ雨▽きか", 11, "▼機会");
        assert_eq!(fresh, ("明日は校明日カ雨".into(), Origin::PreeditRemoved));
    }

    #[test]
    fn 前の_surrounding_text_より前に表示した未確定文字列は突き合わせない() {
        let mut trail = PreeditTrail::default();
        trail.displayed("▽あ");
        trail.displayed("");
        trail.left_context(Surrounding {
            text: "明日",
            cursor: 2,
        });
        let left = trail.left_context(Surrounding {
            text: "明日▽あ",
            cursor: 4,
        });
        assert_eq!(left.text, "明日▽あ", "本文に書いた ▽あ を除かない");
    }

    #[test]
    fn 範囲の外のカーソルは末尾として扱う() {
        assert_eq!(left("明日", 9, &[]), ("明日".into(), Origin::Surrounding));
    }

    #[test]
    fn 左文脈は末尾の40字に絞る() {
        let long = "あ".repeat(100) + "精密な";
        let (text, origin) = left(&long, 103, &[]);
        assert_eq!(origin, Origin::Surrounding);
        assert_eq!(text.chars().count(), MAX_LEFT_CONTEXT_CHARS);
        assert!(text.ends_with("精密な"));
    }
}
