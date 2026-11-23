import Cocoa
import InputMethodKit

let connectionName = Bundle.main.infoDictionary?["InputMethodConnectionName"] as? String
guard let connectionName, let bundleIdentifier = Bundle.main.bundleIdentifier else {
    fatalError("Info.plist に InputMethodConnectionName か CFBundleIdentifier が無い")
}
guard let server = IMKServer(name: connectionName, bundleIdentifier: bundleIdentifier) else {
    fatalError("IMKServer を作れない")
}
// IMKServer は入力欄ごとの InputController を作る。プロセスが終わるまで持ち続ける
withExtendedLifetime(server) {
    NSApplication.shared.run()
}
