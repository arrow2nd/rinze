import Foundation

/// 入力モードを切り替えたときに、カーソルの下に短く出すラベル。
func modeLabel(_ mode: RinzeMode) -> String {
    switch mode {
    case RINZE_MODE_HIRAGANA: return "かな"
    case RINZE_MODE_KATAKANA: return "カナ"
    case RINZE_MODE_HALFWIDTH_KATAKANA: return "ｶﾅ"
    case RINZE_MODE_FULLWIDTH_LATIN: return "全英"
    case RINZE_MODE_ASCII: return "aA"
    default: return ""
    }
}
