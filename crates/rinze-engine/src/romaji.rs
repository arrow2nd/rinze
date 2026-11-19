#[rustfmt::skip]
const TABLE: &[(&str, &str)] = &[
    ("a", "あ"), ("i", "い"), ("u", "う"), ("e", "え"), ("o", "お"),
    ("ka", "か"), ("ki", "き"), ("ku", "く"), ("ke", "け"), ("ko", "こ"),
    ("sa", "さ"), ("si", "し"), ("su", "す"), ("se", "せ"), ("so", "そ"),
    ("ta", "た"), ("ti", "ち"), ("tu", "つ"), ("te", "て"), ("to", "と"),
    ("na", "な"), ("ni", "に"), ("nu", "ぬ"), ("ne", "ね"), ("no", "の"),
    ("ha", "は"), ("hi", "ひ"), ("hu", "ふ"), ("he", "へ"), ("ho", "ほ"),
    ("ma", "ま"), ("mi", "み"), ("mu", "む"), ("me", "め"), ("mo", "も"),
    ("ya", "や"), ("yi", "い"), ("yu", "ゆ"), ("ye", "いぇ"), ("yo", "よ"),
    ("ra", "ら"), ("ri", "り"), ("ru", "る"), ("re", "れ"), ("ro", "ろ"),
    ("wa", "わ"), ("wi", "うぃ"), ("wu", "う"), ("we", "うぇ"), ("wo", "を"),
    ("ga", "が"), ("gi", "ぎ"), ("gu", "ぐ"), ("ge", "げ"), ("go", "ご"),
    ("za", "ざ"), ("zi", "じ"), ("zu", "ず"), ("ze", "ぜ"), ("zo", "ぞ"),
    ("da", "だ"), ("di", "ぢ"), ("du", "づ"), ("de", "で"), ("do", "ど"),
    ("ba", "ば"), ("bi", "び"), ("bu", "ぶ"), ("be", "べ"), ("bo", "ぼ"),
    ("pa", "ぱ"), ("pi", "ぴ"), ("pu", "ぷ"), ("pe", "ぺ"), ("po", "ぽ"),
    ("kya", "きゃ"), ("kyi", "きぃ"), ("kyu", "きゅ"), ("kye", "きぇ"), ("kyo", "きょ"),
    ("sya", "しゃ"), ("syi", "しぃ"), ("syu", "しゅ"), ("sye", "しぇ"), ("syo", "しょ"),
    ("tya", "ちゃ"), ("tyi", "ちぃ"), ("tyu", "ちゅ"), ("tye", "ちぇ"), ("tyo", "ちょ"),
    ("nya", "にゃ"), ("nyi", "にぃ"), ("nyu", "にゅ"), ("nye", "にぇ"), ("nyo", "にょ"),
    ("hya", "ひゃ"), ("hyi", "ひぃ"), ("hyu", "ひゅ"), ("hye", "ひぇ"), ("hyo", "ひょ"),
    ("mya", "みゃ"), ("myi", "みぃ"), ("myu", "みゅ"), ("mye", "みぇ"), ("myo", "みょ"),
    ("rya", "りゃ"), ("ryi", "りぃ"), ("ryu", "りゅ"), ("rye", "りぇ"), ("ryo", "りょ"),
    ("gya", "ぎゃ"), ("gyi", "ぎぃ"), ("gyu", "ぎゅ"), ("gye", "ぎぇ"), ("gyo", "ぎょ"),
    ("zya", "じゃ"), ("zyi", "じぃ"), ("zyu", "じゅ"), ("zye", "じぇ"), ("zyo", "じょ"),
    ("dya", "ぢゃ"), ("dyi", "ぢぃ"), ("dyu", "ぢゅ"), ("dye", "ぢぇ"), ("dyo", "ぢょ"),
    ("bya", "びゃ"), ("byi", "びぃ"), ("byu", "びゅ"), ("bye", "びぇ"), ("byo", "びょ"),
    ("pya", "ぴゃ"), ("pyi", "ぴぃ"), ("pyu", "ぴゅ"), ("pye", "ぴぇ"), ("pyo", "ぴょ"),
    ("sha", "しゃ"), ("shi", "し"), ("shu", "しゅ"), ("she", "しぇ"), ("sho", "しょ"),
    ("cha", "ちゃ"), ("chi", "ち"), ("chu", "ちゅ"), ("che", "ちぇ"), ("cho", "ちょ"),
    ("tsu", "つ"),
    ("ja", "じゃ"), ("ji", "じ"), ("ju", "じゅ"), ("je", "じぇ"), ("jo", "じょ"),
    ("fa", "ふぁ"), ("fi", "ふぃ"), ("fu", "ふ"), ("fe", "ふぇ"), ("fo", "ふぉ"),
    ("va", "ゔぁ"), ("vi", "ゔぃ"), ("vu", "ゔ"), ("ve", "ゔぇ"), ("vo", "ゔぉ"),
    ("tha", "てゃ"), ("thi", "てぃ"), ("thu", "てゅ"), ("the", "てぇ"), ("tho", "てょ"),
    ("dha", "でゃ"), ("dhi", "でぃ"), ("dhu", "でゅ"), ("dhe", "でぇ"), ("dho", "でょ"),
    ("xa", "ぁ"), ("xi", "ぃ"), ("xu", "ぅ"), ("xe", "ぇ"), ("xo", "ぉ"),
    ("xya", "ゃ"), ("xyu", "ゅ"), ("xyo", "ょ"), ("xtu", "っ"), ("xtsu", "っ"), ("xwa", "ゎ"),
    ("xka", "ゕ"), ("xke", "ゖ"),
    ("nn", "ん"), ("n'", "ん"),
    ("-", "ー"), (",", "、"), (".", "。"), ("?", "？"), ("!", "！"), (":", "："), (";", "；"),
    ("[", "「"), ("]", "」"), ("~", "〜"),
    ("z,", "‥"), ("z-", "〜"), ("z.", "…"), ("z/", "・"), ("z[", "『"), ("z]", "』"),
    ("zh", "←"), ("zj", "↓"), ("zk", "↑"), ("zl", "→"),
];

fn lookup(roman: &str) -> Option<&'static str> {
    TABLE.iter().find(|(r, _)| *r == roman).map(|(_, k)| *k)
}

fn is_prefix(roman: &str) -> bool {
    TABLE
        .iter()
        .any(|(r, _)| r.len() > roman.len() && r.starts_with(roman))
}

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

    pub fn feed(&mut self, c: char) -> String {
        let mut out = String::new();
        let mut joined = self.pending.clone();
        joined.push(c);
        if let Some(kana) = lookup(&joined) {
            self.pending.clear();
            out.push_str(kana);
        } else if is_prefix(&joined) {
            self.pending = joined;
        } else if self.pending == "n" {
            out.push('ん');
            self.pending.clear();
            out.push_str(&self.feed(c));
        } else if self.pending.len() == 1 && self.pending.starts_with(c) && is_sokuon_consonant(c) {
            out.push('っ');
        } else if !self.pending.is_empty() {
            self.pending.clear();
            out.push_str(&self.feed(c));
        } else {
            out.push(c);
        }
        out
    }

    pub fn flush(&mut self) -> String {
        let out = if self.pending == "n" { "ん" } else { "" };
        self.pending.clear();
        out.to_string()
    }

    pub fn backspace(&mut self) -> bool {
        self.pending.pop().is_some()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

fn is_sokuon_consonant(c: char) -> bool {
    c.is_ascii_lowercase() && !"aiueon".contains(c)
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
    }
}
