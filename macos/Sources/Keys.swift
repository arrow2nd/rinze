import Cocoa

enum KeyCode {
    static let enter: UInt16 = 36
    static let space: UInt16 = 49
    static let backspace: UInt16 = 51
    static let escape: UInt16 = 53
    static let keypadEnter: UInt16 = 76
    static let down: UInt16 = 125
    static let up: UInt16 = 126
}

/// 打鍵を rinzed のキーの名前にする。rinzed が扱わないキーは nil。
func keyName(
    keyCode: UInt16, characters: String?, charactersIgnoringModifiers: String?,
    modifiers: NSEvent.ModifierFlags
) -> String? {
    let modifiers = modifiers.intersection([.control, .command, .option])
    if modifiers == .control {
        switch charactersIgnoringModifiers?.lowercased() {
        case "g": return "ctrl-g"
        case "j": return "ctrl-j"
        case "q": return "ctrl-q"
        default: return nil
        }
    }
    if !modifiers.isEmpty {
        return nil
    }
    switch keyCode {
    case KeyCode.space: return "space"
    case KeyCode.enter, KeyCode.keypadEnter: return "enter"
    case KeyCode.backspace: return "backspace"
    default: break
    }
    guard let characters, characters.unicodeScalars.count == 1,
        let scalar = characters.unicodeScalars.first, (0x21...0x7e).contains(scalar.value)
    else {
        return nil
    }
    return characters
}

func keyName(_ event: NSEvent) -> String? {
    keyName(
        keyCode: event.keyCode, characters: event.characters,
        charactersIgnoringModifiers: event.charactersIgnoringModifiers,
        modifiers: event.modifierFlags)
}

/// カーソルより前の文字列を取る範囲。`end` はカーソルか未確定文字列の先頭で、UTF-16 の位置。
func leftRange(end: Int, window: Int) -> NSRange {
    let start = max(0, end - window)
    return NSRange(location: start, length: end - start)
}
