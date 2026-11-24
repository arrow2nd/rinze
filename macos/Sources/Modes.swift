import Foundation

/// 入力モード 1 つ。`source` は Info.plist の tsInputModeListKey のキーから、バンドルの識別子を除いた末尾。
struct ModeInfo {
    let mode: RinzeMode
    let source: String
    let label: String
}

let modes: [ModeInfo] = [
    ModeInfo(mode: RINZE_MODE_HIRAGANA, source: "hiragana", label: "かな"),
    ModeInfo(mode: RINZE_MODE_KATAKANA, source: "katakana", label: "カナ"),
    ModeInfo(mode: RINZE_MODE_HALFWIDTH_KATAKANA, source: "halfwidth-katakana", label: "ｶﾅ"),
    ModeInfo(mode: RINZE_MODE_FULLWIDTH_LATIN, source: "fullwidth-latin", label: "全英"),
    ModeInfo(mode: RINZE_MODE_ASCII, source: "ascii", label: "aA"),
]

func modeInfo(_ mode: RinzeMode) -> ModeInfo? {
    modes.first { $0.mode == mode }
}

/// 入力ソースの識別子 (`<バンドルの識別子>.<source>`) から入力モードを引く。
func modeInfo(sourceID: String, bundleIdentifier: String) -> ModeInfo? {
    let prefix = bundleIdentifier + "."
    guard sourceID.hasPrefix(prefix) else {
        return nil
    }
    let source = sourceID.dropFirst(prefix.count)
    return modes.first { $0.source == source }
}
