// ddskk の `skk-rom-kana-base-rule-list` と `skk-rom-kana-rule-list` の既定値のうち、かなと記号を出す規則。
// 各行は ddskk と同じく (入力, 次の状態, 出力)。関数を呼ぶ規則 (`q` `l` `/` など) は KanaKey で扱う。
//
// ddskk との違い:
// - `va` などは ddskk の「う゛ぁ」ではなく「ゔぁ」、`xka` `xke` は ddskk のひらがなモードの「か」「け」ではなく「ゕ」「ゖ」を出す。
//   カタカナモードではどちらも ddskk と同じ「ヴァ」「ヵ」「ヶ」になる。
// - `@` (`skk-today`)、`$` (`skk-display-code-for-char-at-point`)、`\` (`skk-input-by-code-or-menu`) は持たず、そのまま入る。
#[rustfmt::skip]
const RULES: &[(&str, &str, &str)] = &[
    ("a", "", "あ"), ("i", "", "い"), ("u", "", "う"), ("e", "", "え"), ("o", "", "お"),
    ("ka", "", "か"), ("ki", "", "き"), ("ku", "", "く"), ("ke", "", "け"), ("ko", "", "こ"),
    ("sa", "", "さ"), ("si", "", "し"), ("su", "", "す"), ("se", "", "せ"), ("so", "", "そ"),
    ("ta", "", "た"), ("ti", "", "ち"), ("tu", "", "つ"), ("te", "", "て"), ("to", "", "と"),
    ("na", "", "な"), ("ni", "", "に"), ("nu", "", "ぬ"), ("ne", "", "ね"), ("no", "", "の"),
    ("ha", "", "は"), ("hi", "", "ひ"), ("hu", "", "ふ"), ("he", "", "へ"), ("ho", "", "ほ"),
    ("ma", "", "ま"), ("mi", "", "み"), ("mu", "", "む"), ("me", "", "め"), ("mo", "", "も"),
    ("ya", "", "や"), ("yu", "", "ゆ"), ("ye", "", "いぇ"), ("yo", "", "よ"),
    ("ra", "", "ら"), ("ri", "", "り"), ("ru", "", "る"), ("re", "", "れ"), ("ro", "", "ろ"),
    ("wa", "", "わ"), ("wi", "", "うぃ"), ("wu", "", "う"), ("we", "", "うぇ"), ("wo", "", "を"),
    ("ga", "", "が"), ("gi", "", "ぎ"), ("gu", "", "ぐ"), ("ge", "", "げ"), ("go", "", "ご"),
    ("za", "", "ざ"), ("zi", "", "じ"), ("zu", "", "ず"), ("ze", "", "ぜ"), ("zo", "", "ぞ"),
    ("da", "", "だ"), ("di", "", "ぢ"), ("du", "", "づ"), ("de", "", "で"), ("do", "", "ど"),
    ("ba", "", "ば"), ("bi", "", "び"), ("bu", "", "ぶ"), ("be", "", "べ"), ("bo", "", "ぼ"),
    ("pa", "", "ぱ"), ("pi", "", "ぴ"), ("pu", "", "ぷ"), ("pe", "", "ぺ"), ("po", "", "ぽ"),
    ("kya", "", "きゃ"), ("kyi", "", "きぃ"), ("kyu", "", "きゅ"), ("kye", "", "きぇ"), ("kyo", "", "きょ"),
    ("sya", "", "しゃ"), ("syi", "", "しぃ"), ("syu", "", "しゅ"), ("sye", "", "しぇ"), ("syo", "", "しょ"),
    ("tya", "", "ちゃ"), ("tyi", "", "ちぃ"), ("tyu", "", "ちゅ"), ("tye", "", "ちぇ"), ("tyo", "", "ちょ"),
    ("cya", "", "ちゃ"), ("cyi", "", "ちぃ"), ("cyu", "", "ちゅ"), ("cye", "", "ちぇ"), ("cyo", "", "ちょ"),
    ("nya", "", "にゃ"), ("nyi", "", "にぃ"), ("nyu", "", "にゅ"), ("nye", "", "にぇ"), ("nyo", "", "にょ"),
    ("hya", "", "ひゃ"), ("hyi", "", "ひぃ"), ("hyu", "", "ひゅ"), ("hye", "", "ひぇ"), ("hyo", "", "ひょ"),
    ("fya", "", "ふゃ"), ("fyi", "", "ふぃ"), ("fyu", "", "ふゅ"), ("fye", "", "ふぇ"), ("fyo", "", "ふょ"),
    ("mya", "", "みゃ"), ("myi", "", "みぃ"), ("myu", "", "みゅ"), ("mye", "", "みぇ"), ("myo", "", "みょ"),
    ("rya", "", "りゃ"), ("ryi", "", "りぃ"), ("ryu", "", "りゅ"), ("rye", "", "りぇ"), ("ryo", "", "りょ"),
    ("gya", "", "ぎゃ"), ("gyi", "", "ぎぃ"), ("gyu", "", "ぎゅ"), ("gye", "", "ぎぇ"), ("gyo", "", "ぎょ"),
    ("zya", "", "じゃ"), ("zyi", "", "じぃ"), ("zyu", "", "じゅ"), ("zye", "", "じぇ"), ("zyo", "", "じょ"),
    ("jya", "", "じゃ"), ("jyi", "", "じぃ"), ("jyu", "", "じゅ"), ("jye", "", "じぇ"), ("jyo", "", "じょ"),
    ("dya", "", "ぢゃ"), ("dyi", "", "ぢぃ"), ("dyu", "", "ぢゅ"), ("dye", "", "ぢぇ"), ("dyo", "", "ぢょ"),
    ("bya", "", "びゃ"), ("byi", "", "びぃ"), ("byu", "", "びゅ"), ("bye", "", "びぇ"), ("byo", "", "びょ"),
    ("pya", "", "ぴゃ"), ("pyi", "", "ぴぃ"), ("pyu", "", "ぴゅ"), ("pye", "", "ぴぇ"), ("pyo", "", "ぴょ"),
    ("sha", "", "しゃ"), ("shi", "", "し"), ("shu", "", "しゅ"), ("she", "", "しぇ"), ("sho", "", "しょ"),
    ("cha", "", "ちゃ"), ("chi", "", "ち"), ("chu", "", "ちゅ"), ("che", "", "ちぇ"), ("cho", "", "ちょ"),
    ("tsu", "", "つ"),
    ("ja", "", "じゃ"), ("ji", "", "じ"), ("ju", "", "じゅ"), ("je", "", "じぇ"), ("jo", "", "じょ"),
    ("fa", "", "ふぁ"), ("fi", "", "ふぃ"), ("fu", "", "ふ"), ("fe", "", "ふぇ"), ("fo", "", "ふぉ"),
    ("va", "", "ゔぁ"), ("vi", "", "ゔぃ"), ("vu", "", "ゔ"), ("ve", "", "ゔぇ"), ("vo", "", "ゔぉ"),
    ("tha", "", "てぁ"), ("thi", "", "てぃ"), ("thu", "", "てゅ"), ("the", "", "てぇ"), ("tho", "", "てょ"),
    ("dha", "", "でゃ"), ("dhi", "", "でぃ"), ("dhu", "", "でゅ"), ("dhe", "", "でぇ"), ("dho", "", "でょ"),
    ("xa", "", "ぁ"), ("xi", "", "ぃ"), ("xu", "", "ぅ"), ("xe", "", "ぇ"), ("xo", "", "ぉ"),
    ("xya", "", "ゃ"), ("xyu", "", "ゅ"), ("xyo", "", "ょ"), ("xtu", "", "っ"), ("xtsu", "", "っ"),
    ("xwa", "", "ゎ"), ("xwi", "", "ゐ"), ("xwe", "", "ゑ"), ("xka", "", "ゕ"), ("xke", "", "ゖ"),
    ("n", "", "ん"), ("nn", "", "ん"), ("n'", "", "ん"), ("mm", "m", "ん"),
    ("bb", "b", "っ"), ("cc", "c", "っ"), ("dd", "d", "っ"), ("ff", "f", "っ"), ("gg", "g", "っ"),
    ("hh", "h", "っ"), ("jj", "j", "っ"), ("kk", "k", "っ"), ("pp", "p", "っ"), ("rr", "r", "っ"),
    ("ss", "s", "っ"), ("tt", "t", "っ"), ("vv", "v", "っ"), ("ww", "w", "っ"), ("xx", "x", "っ"),
    ("yy", "y", "っ"), ("zz", "z", "っ"),
    ("-", "", "ー"), (",", "", "、"), (".", "", "。"), ("?", "", "？"), (":", "", "："), (";", "", "；"),
    ("[", "", "「"), ("]", "", "」"),
    ("z ", "", "　"), ("z*", "", "※"), ("z,", "", "‥"), ("z-", "", "〜"), ("z.", "", "…"), ("z/", "", "・"),
    ("z0", "", "○"), ("z:", "", "゜"), ("z;", "", "゛"), ("z@", "", "◎"), ("z[", "", "『"), ("z]", "", "』"),
    ("z{", "", "【"), ("z}", "", "】"), ("z(", "", "（"), ("z)", "", "）"),
    ("zh", "", "←"), ("zj", "", "↓"), ("zk", "", "↑"), ("zl", "", "→"), ("zL", "", "⇒"), ("zn", "", "ー"),
];

fn rule(roman: &str) -> Option<(&'static str, &'static str)> {
    RULES
        .iter()
        .find(|(input, ..)| *input == roman)
        .map(|&(_, next, output)| (next, output))
}

fn has_longer_rule(roman: &str) -> bool {
    RULES
        .iter()
        .any(|(input, ..)| input.len() > roman.len() && input.starts_with(roman))
}

// ddskk と同じく、より長い規則に続きうる間は出力を保留し、
// 続かない文字が来たら保留中のものが規則ならその出力を出し、規則でなければ捨てる。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Romaji {
    pending: String,
}

impl Romaji {
    pub fn pending(&self) -> &str {
        &self.pending
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn continues(&self, c: char) -> bool {
        let joined = format!("{}{c}", self.pending);
        has_longer_rule(&joined) || rule(&joined).is_some()
    }

    pub fn feed(&mut self, c: char) -> String {
        let joined = format!("{}{c}", self.pending);
        if has_longer_rule(&joined) {
            self.pending = joined;
            return String::new();
        }
        if let Some((next, output)) = rule(&joined) {
            self.pending = next.to_string();
            return output.to_string();
        }
        if self.pending.is_empty() {
            return c.to_string();
        }
        let mut out = self.flush().to_string();
        out.push_str(&self.feed(c));
        out
    }

    pub fn flush(&mut self) -> &'static str {
        let output = rule(&self.pending).map_or("", |(_, output)| output);
        self.pending.clear();
        output
    }

    pub fn backspace(&mut self) -> bool {
        self.pending.pop().is_some()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_all(s: &str) -> (String, String) {
        let mut r = Romaji::default();
        let out: String = s.chars().map(|c| r.feed(c)).collect();
        (out, r.pending().to_string())
    }

    #[test]
    fn 基本の変換() {
        assert_eq!(
            type_all("koredeaimasuka?"),
            ("これであいますか？".into(), "".into())
        );
        assert_eq!(type_all("mayone-zu"), ("まよねーず".into(), "".into()));
        assert_eq!(type_all("kyoushitsu"), ("きょうしつ".into(), "".into()));
    }

    #[test]
    fn 促音は同じ子音の連続で作る() {
        assert_eq!(type_all("atte"), ("あって".into(), "".into()));
        assert_eq!(type_all("at"), ("あ".into(), "t".into()));
        assert_eq!(type_all("att"), ("あっ".into(), "t".into()));
    }

    #[test]
    fn nの扱い() {
        assert_eq!(type_all("kanji"), ("かんじ".into(), "".into()));
        assert_eq!(type_all("konnnichiha"), ("こんにちは".into(), "".into()));
        assert_eq!(type_all("kon'ya"), ("こんや".into(), "".into()));
        assert_eq!(type_all("konya"), ("こにゃ".into(), "".into()));
        assert_eq!(type_all("hon"), ("ほ".into(), "n".into()));
        let mut r = Romaji::default();
        "hon".chars().for_each(|c| {
            r.feed(c);
        });
        assert_eq!(r.flush(), "ん");
    }

    #[test]
    fn 続かない子音は捨てる() {
        assert_eq!(type_all("kta"), ("た".into(), "".into()));
        assert_eq!(type_all("1"), ("1".into(), "".into()));
        let mut r = Romaji::default();
        r.feed('k');
        assert_eq!(r.flush(), "", "規則でない保留は確定時に捨てる");
    }
}
