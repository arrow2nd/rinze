import Carbon
import Cocoa
import InputMethodKit

let connectionName = Bundle.main.infoDictionary?["InputMethodConnectionName"] as? String
guard let connectionName, let bundleIdentifier = Bundle.main.bundleIdentifier else {
    fatalError("Info.plist に InputMethodConnectionName か CFBundleIdentifier が無い")
}

// `rinze --register` は、このアプリの入力ソースを macOS に登録し、すぐ終わる。
// ログインし直さなくても、入れた直後からシステム設定の入力ソースの一覧に出る。
// 有効にするのは利用者がシステム設定でする。TISEnableInputSource で有効にしても、macOS は設定に残さない
if CommandLine.arguments.contains("--register") {
    let status = TISRegisterInputSource(Bundle.main.bundleURL as CFURL)
    guard status == noErr else {
        FileHandle.standardError.write(Data("rinze: 入力ソースを登録できない (\(status))\n".utf8))
        exit(1)
    }
    exit(0)
}

guard let server = IMKServer(name: connectionName, bundleIdentifier: bundleIdentifier) else {
    fatalError("IMKServer を作れない")
}
// IMKServer は入力欄ごとの InputController を作る。プロセスが終わるまで持ち続ける
withExtendedLifetime(server) {
    NSApplication.shared.run()
}
