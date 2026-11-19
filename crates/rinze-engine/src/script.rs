const HIRAGANA_TO_KATAKANA: u32 = 0x60;
const FULLWIDTH_ASCII_OFFSET: u32 = 0xFEE0;

pub fn katakana(hiragana: &str) -> String {
    hiragana.chars().map(katakana_char).collect()
}

fn katakana_char(c: char) -> char {
    match c {
        'ぁ'..='ゖ' | 'ゝ' | 'ゞ' => {
            char::from_u32(c as u32 + HIRAGANA_TO_KATAKANA).unwrap_or(c)
        }
        _ => c,
    }
}

pub fn halfwidth_katakana(kana: &str) -> String {
    let mut out = String::new();
    for c in kana.chars().map(katakana_char) {
        match HALFWIDTH.iter().find(|(full, _)| *full == c) {
            Some((_, half)) => out.push_str(half),
            None => out.push(fullwidth_ascii_to_ascii(c).unwrap_or(c)),
        }
    }
    out
}

fn fullwidth_ascii_to_ascii(c: char) -> Option<char> {
    matches!(c, '！'..='～')
        .then(|| char::from_u32(c as u32 - FULLWIDTH_ASCII_OFFSET))
        .flatten()
}

#[rustfmt::skip]
const HALFWIDTH: &[(char, &str)] = &[
    ('ァ', "ｧ"), ('ア', "ｱ"), ('ィ', "ｨ"), ('イ', "ｲ"), ('ゥ', "ｩ"), ('ウ', "ｳ"), ('ェ', "ｪ"), ('エ', "ｴ"), ('ォ', "ｫ"), ('オ', "ｵ"),
    ('カ', "ｶ"), ('ガ', "ｶﾞ"), ('キ', "ｷ"), ('ギ', "ｷﾞ"), ('ク', "ｸ"), ('グ', "ｸﾞ"), ('ケ', "ｹ"), ('ゲ', "ｹﾞ"), ('コ', "ｺ"), ('ゴ', "ｺﾞ"),
    ('サ', "ｻ"), ('ザ', "ｻﾞ"), ('シ', "ｼ"), ('ジ', "ｼﾞ"), ('ス', "ｽ"), ('ズ', "ｽﾞ"), ('セ', "ｾ"), ('ゼ', "ｾﾞ"), ('ソ', "ｿ"), ('ゾ', "ｿﾞ"),
    ('タ', "ﾀ"), ('ダ', "ﾀﾞ"), ('チ', "ﾁ"), ('ヂ', "ﾁﾞ"), ('ッ', "ｯ"), ('ツ', "ﾂ"), ('ヅ', "ﾂﾞ"), ('テ', "ﾃ"), ('デ', "ﾃﾞ"), ('ト', "ﾄ"), ('ド', "ﾄﾞ"),
    ('ナ', "ﾅ"), ('ニ', "ﾆ"), ('ヌ', "ﾇ"), ('ネ', "ﾈ"), ('ノ', "ﾉ"),
    ('ハ', "ﾊ"), ('バ', "ﾊﾞ"), ('パ', "ﾊﾟ"), ('ヒ', "ﾋ"), ('ビ', "ﾋﾞ"), ('ピ', "ﾋﾟ"), ('フ', "ﾌ"), ('ブ', "ﾌﾞ"), ('プ', "ﾌﾟ"),
    ('ヘ', "ﾍ"), ('ベ', "ﾍﾞ"), ('ペ', "ﾍﾟ"), ('ホ', "ﾎ"), ('ボ', "ﾎﾞ"), ('ポ', "ﾎﾟ"),
    ('マ', "ﾏ"), ('ミ', "ﾐ"), ('ム', "ﾑ"), ('メ', "ﾒ"), ('モ', "ﾓ"),
    ('ャ', "ｬ"), ('ヤ', "ﾔ"), ('ュ', "ｭ"), ('ユ', "ﾕ"), ('ョ', "ｮ"), ('ヨ', "ﾖ"),
    ('ラ', "ﾗ"), ('リ', "ﾘ"), ('ル', "ﾙ"), ('レ', "ﾚ"), ('ロ', "ﾛ"),
    ('ヮ', "ﾜ"), ('ワ', "ﾜ"), ('ヰ', "ｲ"), ('ヱ', "ｴ"), ('ヲ', "ｦ"), ('ン', "ﾝ"),
    ('ヴ', "ｳﾞ"), ('ヵ', "ｶ"), ('ヶ', "ｹ"),
    ('。', "｡"), ('「', "｢"), ('」', "｣"), ('、', "､"), ('・', "･"), ('ー', "ｰ"), ('゛', "ﾞ"), ('゜', "ﾟ"),
];

// ddskk の `skk-jisx0208-latin-vector` の既定値と同じ表。`"` `'` `-` `` ` `` `~` は ASCII の全角形ではない文字になる。
const FULLWIDTH_LATIN: &str = "　！”＃＄％＆’（）＊＋，−．／０１２３４５６７８９：；＜＝＞？＠ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺ［＼］＾＿‘ａｂｃｄｅｆｇｈｉｊｋｌｍｎｏｐｑｒｓｔｕｖｗｘｙｚ｛｜｝〜";

pub fn fullwidth_latin(c: char) -> Option<char> {
    let index = (c as u32).checked_sub(' ' as u32)?;
    FULLWIDTH_LATIN.chars().nth(index as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ひらがなをカタカナにする() {
        assert_eq!(katakana("ゔぁいおりん、ゕゖ"), "ヴァイオリン、ヵヶ");
        assert_eq!(katakana("漢字かな1"), "漢字カナ1");
    }

    #[test]
    fn 半角カナの濁点と半濁点は2文字になる() {
        assert_eq!(halfwidth_katakana("がっぱゔ"), "ｶﾞｯﾊﾟｳﾞ");
        assert_eq!(halfwidth_katakana("カタカナ"), "ｶﾀｶﾅ");
        assert_eq!(halfwidth_katakana("「らーめん」、。"), "｢ﾗｰﾒﾝ｣､｡");
        assert_eq!(
            halfwidth_katakana("？！：；1〜"),
            "?!:;1〜",
            "全角の英数記号は半角にし、対応の無い文字はそのまま"
        );
    }

    #[test]
    fn 全角英字はddskkの表に従う() {
        let typed: String = "Ab1 -~\"".chars().filter_map(fullwidth_latin).collect();
        assert_eq!(typed, "Ａｂ１　−〜”");
        assert_eq!(fullwidth_latin('\u{7f}'), None);
        assert_eq!(fullwidth_latin('あ'), None);
    }
}
