// rinzed にも入力欄にも触れずに確かめられる部分のテスト。macos/test.sh が組んで走らせる
import Cocoa

var failures = 0

func expect<T: Equatable>(_ actual: T, _ expected: T, _ what: String, line: UInt = #line) {
    if actual != expected {
        failures += 1
        print("\(line) 行: \(what): \(expected) のはずが \(actual)")
    }
}

func key(
    _ keyCode: UInt16, _ characters: String?, ignoring: String? = nil,
    _ modifiers: NSEvent.ModifierFlags = []
) -> String? {
    keyName(
        keyCode: keyCode, characters: characters,
        charactersIgnoringModifiers: ignoring ?? characters, modifiers: modifiers)
}

// 印字できる ASCII は字のまま送る。Shift は字に入っているので見ない
expect(key(0, "a"), "a", "小文字")
expect(key(0, "A", ignoring: "a", .shift), "A", "大文字")
expect(key(44, "/"), "/", "記号")
expect(key(18, "!", ignoring: "1", .shift), "!", "Shift の付いた記号")
expect(key(0, "~"), "~", "印字できる ASCII の終わり")
// CapsLock やテンキーの印は打鍵の意味を変えない
expect(key(0, "a", [.capsLock, .numericPad]), "a", "見ない修飾")

// 名前で送るキー
expect(key(KeyCode.space, " "), "space", "Space")
expect(key(KeyCode.enter, "\r"), "enter", "Enter")
expect(key(KeyCode.keypadEnter, "\u{3}"), "enter", "テンキーの Enter")
expect(key(KeyCode.backspace, "\u{7f}"), "backspace", "BackSpace")

// Ctrl は rinzed が扱う 3 つだけを送る
expect(key(38, "\n", ignoring: "j", .control), "ctrl-j", "C-j")
expect(key(5, "\u{7}", ignoring: "g", .control), "ctrl-g", "C-g")
expect(key(12, "\u{11}", ignoring: "q", .control), "ctrl-q", "C-q")
expect(key(38, "\n", ignoring: "J", [.control, .shift]), "ctrl-j", "Shift の付いた C-j")
expect(key(0, "\u{1}", ignoring: "a", .control), nil, "ほかの Ctrl")

// アプリのショートカットや rinzed が扱わないキーは送らない
expect(key(0, "a", .command), nil, "Cmd")
expect(key(0, "å", ignoring: "a", .option), nil, "Option")
expect(key(38, "\n", ignoring: "j", [.control, .command]), nil, "Ctrl と Cmd")
expect(key(KeyCode.escape, "\u{1b}"), nil, "Escape")
expect(key(KeyCode.up, "\u{F700}"), nil, "矢印")
expect(key(48, "\t"), nil, "Tab")
expect(key(0, "あ"), nil, "ASCII でない字")
expect(key(0, "ab"), nil, "2 字")
expect(key(0, nil), nil, "字が無い")

// 左文脈を取る範囲は、カーソルの手前の window 個まで
expect(leftRange(end: 100, window: 80), NSRange(location: 20, length: 80), "window より長い")
expect(leftRange(end: 30, window: 80), NSRange(location: 0, length: 30), "window より短い")
expect(leftRange(end: 0, window: 80), NSRange(location: 0, length: 0), "先頭")

// 入力モードのラベル
expect(modeLabel(RINZE_MODE_HIRAGANA), "かな", "ひらがな")
expect(modeLabel(RINZE_MODE_KATAKANA), "カナ", "カタカナ")
expect(modeLabel(RINZE_MODE_HALFWIDTH_KATAKANA), "ｶﾅ", "半角カナ")
expect(modeLabel(RINZE_MODE_FULLWIDTH_LATIN), "全英", "全角英字")
expect(modeLabel(RINZE_MODE_ASCII), "aA", "ASCII")

// Info.plist の指すアイコンがある
let plist = NSDictionary(contentsOfFile: CommandLine.arguments[1])!
let icon = plist["tsInputMethodIconFileKey"] as! String
expect(
    FileManager.default.fileExists(atPath: "\(CommandLine.arguments[2])/\(icon)"), true,
    "\(icon) がある")

if failures > 0 {
    print("\(failures) 件の失敗")
    exit(1)
}
print("ok")
