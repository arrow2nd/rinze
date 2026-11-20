struct Rule {
    input: &'static str,
    output: &'static str,
    pending: &'static str,
}

const fn emit(input: &'static str, output: &'static str) -> Rule {
    emit_keep(input, output, "")
}

const fn emit_keep(input: &'static str, output: &'static str, pending: &'static str) -> Rule {
    Rule {
        input,
        output,
        pending,
    }
}

#[rustfmt::skip]
const RULES: &[Rule] = &[
    emit("a", "あ"), emit("i", "い"), emit("u", "う"), emit("e", "え"), emit("o", "お"),
    emit("ka", "か"), emit("ki", "き"), emit("ku", "く"), emit("ke", "け"), emit("ko", "こ"),
    emit("sa", "さ"), emit("si", "し"), emit("su", "す"), emit("se", "せ"), emit("so", "そ"),
    emit("ta", "た"), emit("ti", "ち"), emit("tu", "つ"), emit("te", "て"), emit("to", "と"),
    emit("na", "な"), emit("ni", "に"), emit("nu", "ぬ"), emit("ne", "ね"), emit("no", "の"),
    emit("ha", "は"), emit("hi", "ひ"), emit("hu", "ふ"), emit("he", "へ"), emit("ho", "ほ"),
    emit("ma", "ま"), emit("mi", "み"), emit("mu", "む"), emit("me", "め"), emit("mo", "も"),
    emit("ya", "や"), emit("yu", "ゆ"), emit("ye", "いぇ"), emit("yo", "よ"),
    emit("ra", "ら"), emit("ri", "り"), emit("ru", "る"), emit("re", "れ"), emit("ro", "ろ"),
    emit("wa", "わ"), emit("wi", "うぃ"), emit("wu", "う"), emit("we", "うぇ"), emit("wo", "を"),
    emit("ga", "が"), emit("gi", "ぎ"), emit("gu", "ぐ"), emit("ge", "げ"), emit("go", "ご"),
    emit("za", "ざ"), emit("zi", "じ"), emit("zu", "ず"), emit("ze", "ぜ"), emit("zo", "ぞ"),
    emit("da", "だ"), emit("di", "ぢ"), emit("du", "づ"), emit("de", "で"), emit("do", "ど"),
    emit("ba", "ば"), emit("bi", "び"), emit("bu", "ぶ"), emit("be", "べ"), emit("bo", "ぼ"),
    emit("pa", "ぱ"), emit("pi", "ぴ"), emit("pu", "ぷ"), emit("pe", "ぺ"), emit("po", "ぽ"),
    emit("kya", "きゃ"), emit("kyi", "きぃ"), emit("kyu", "きゅ"), emit("kye", "きぇ"), emit("kyo", "きょ"),
    emit("sya", "しゃ"), emit("syi", "しぃ"), emit("syu", "しゅ"), emit("sye", "しぇ"), emit("syo", "しょ"),
    emit("tya", "ちゃ"), emit("tyi", "ちぃ"), emit("tyu", "ちゅ"), emit("tye", "ちぇ"), emit("tyo", "ちょ"),
    emit("cya", "ちゃ"), emit("cyi", "ちぃ"), emit("cyu", "ちゅ"), emit("cye", "ちぇ"), emit("cyo", "ちょ"),
    emit("nya", "にゃ"), emit("nyi", "にぃ"), emit("nyu", "にゅ"), emit("nye", "にぇ"), emit("nyo", "にょ"),
    emit("hya", "ひゃ"), emit("hyi", "ひぃ"), emit("hyu", "ひゅ"), emit("hye", "ひぇ"), emit("hyo", "ひょ"),
    emit("fya", "ふゃ"), emit("fyi", "ふぃ"), emit("fyu", "ふゅ"), emit("fye", "ふぇ"), emit("fyo", "ふょ"),
    emit("mya", "みゃ"), emit("myi", "みぃ"), emit("myu", "みゅ"), emit("mye", "みぇ"), emit("myo", "みょ"),
    emit("rya", "りゃ"), emit("ryi", "りぃ"), emit("ryu", "りゅ"), emit("rye", "りぇ"), emit("ryo", "りょ"),
    emit("gya", "ぎゃ"), emit("gyi", "ぎぃ"), emit("gyu", "ぎゅ"), emit("gye", "ぎぇ"), emit("gyo", "ぎょ"),
    emit("zya", "じゃ"), emit("zyi", "じぃ"), emit("zyu", "じゅ"), emit("zye", "じぇ"), emit("zyo", "じょ"),
    emit("jya", "じゃ"), emit("jyi", "じぃ"), emit("jyu", "じゅ"), emit("jye", "じぇ"), emit("jyo", "じょ"),
    emit("dya", "ぢゃ"), emit("dyi", "ぢぃ"), emit("dyu", "ぢゅ"), emit("dye", "ぢぇ"), emit("dyo", "ぢょ"),
    emit("bya", "びゃ"), emit("byi", "びぃ"), emit("byu", "びゅ"), emit("bye", "びぇ"), emit("byo", "びょ"),
    emit("pya", "ぴゃ"), emit("pyi", "ぴぃ"), emit("pyu", "ぴゅ"), emit("pye", "ぴぇ"), emit("pyo", "ぴょ"),
    emit("sha", "しゃ"), emit("shi", "し"), emit("shu", "しゅ"), emit("she", "しぇ"), emit("sho", "しょ"),
    emit("cha", "ちゃ"), emit("chi", "ち"), emit("chu", "ちゅ"), emit("che", "ちぇ"), emit("cho", "ちょ"),
    emit("tsu", "つ"),
    emit("ja", "じゃ"), emit("ji", "じ"), emit("ju", "じゅ"), emit("je", "じぇ"), emit("jo", "じょ"),
    emit("fa", "ふぁ"), emit("fi", "ふぃ"), emit("fu", "ふ"), emit("fe", "ふぇ"), emit("fo", "ふぉ"),
    emit("va", "ゔぁ"), emit("vi", "ゔぃ"), emit("vu", "ゔ"), emit("ve", "ゔぇ"), emit("vo", "ゔぉ"),
    emit("tha", "てぁ"), emit("thi", "てぃ"), emit("thu", "てゅ"), emit("the", "てぇ"), emit("tho", "てょ"),
    emit("dha", "でゃ"), emit("dhi", "でぃ"), emit("dhu", "でゅ"), emit("dhe", "でぇ"), emit("dho", "でょ"),
    emit("xa", "ぁ"), emit("xi", "ぃ"), emit("xu", "ぅ"), emit("xe", "ぇ"), emit("xo", "ぉ"),
    emit("xya", "ゃ"), emit("xyu", "ゅ"), emit("xyo", "ょ"), emit("xtu", "っ"), emit("xtsu", "っ"),
    emit("xwa", "ゎ"), emit("xwi", "ゐ"), emit("xwe", "ゑ"), emit("xka", "ゕ"), emit("xke", "ゖ"),
    emit("n", "ん"), emit("nn", "ん"), emit("n'", "ん"), emit_keep("mm", "ん", "m"),
    emit_keep("bb", "っ", "b"), emit_keep("cc", "っ", "c"), emit_keep("dd", "っ", "d"), emit_keep("ff", "っ", "f"), emit_keep("gg", "っ", "g"),
    emit_keep("hh", "っ", "h"), emit_keep("jj", "っ", "j"), emit_keep("kk", "っ", "k"), emit_keep("pp", "っ", "p"), emit_keep("rr", "っ", "r"),
    emit_keep("ss", "っ", "s"), emit_keep("tt", "っ", "t"), emit_keep("vv", "っ", "v"), emit_keep("ww", "っ", "w"), emit_keep("xx", "っ", "x"),
    emit_keep("yy", "っ", "y"), emit_keep("zz", "っ", "z"),
    emit("-", "ー"), emit(",", "、"), emit(".", "。"), emit("?", "？"), emit(":", "："), emit(";", "；"),
    emit("[", "「"), emit("]", "」"), emit("!", "！"), emit("~", "〜"),
    emit("z ", "　"), emit("z*", "※"), emit("z,", "‥"), emit("z-", "〜"), emit("z.", "…"), emit("z/", "・"),
    emit("z0", "○"), emit("z:", "゜"), emit("z;", "゛"), emit("z@", "◎"), emit("z[", "『"), emit("z]", "』"),
    emit("z{", "【"), emit("z}", "】"), emit("z(", "（"), emit("z)", "）"),
    emit("zh", "←"), emit("zj", "↓"), emit("zk", "↑"), emit("zl", "→"), emit("zL", "⇒"), emit("zn", "ー"),
];

fn rule(roman: &str) -> Option<&'static Rule> {
    RULES.iter().find(|rule| rule.input == roman)
}

fn has_longer_rule(roman: &str) -> bool {
    RULES
        .iter()
        .any(|rule| rule.input.len() > roman.len() && rule.input.starts_with(roman))
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Romaji {
    pending: String,
}

impl Romaji {
    pub(crate) fn pending(&self) -> &str {
        &self.pending
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(crate) fn continues(&self, c: char) -> bool {
        let joined = format!("{}{c}", self.pending);
        has_longer_rule(&joined) || rule(&joined).is_some()
    }

    pub(crate) fn feed(&mut self, c: char) -> String {
        let joined = format!("{}{c}", self.pending);
        if has_longer_rule(&joined) {
            self.pending = joined;
            return String::new();
        }
        if let Some(rule) = rule(&joined) {
            self.pending = rule.pending.to_string();
            return rule.output.to_string();
        }
        if self.pending.is_empty() {
            return c.to_string();
        }
        let mut out = self.flush().to_string();
        out.push_str(&self.feed(c));
        out
    }

    pub(crate) fn flush(&mut self) -> &'static str {
        let output = rule(&self.pending).map_or("", |rule| rule.output);
        self.pending.clear();
        output
    }

    pub(crate) fn backspace(&mut self) -> bool {
        self.pending.pop().is_some()
    }

    pub(crate) fn clear(&mut self) {
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
