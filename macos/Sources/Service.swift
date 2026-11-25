import Foundation

/// rinzed の待ち受け 1 つ。プロセスに 1 つだけ作り、入力欄ごとの接続はここから作る。
final class Service {
    static let shared = Service()

    let handle: OpaquePointer
    let debug: Bool
    let directApps = DirectApps(defaults: .standard)

    private init() {
        let defaults = UserDefaults.standard
        let socket = defaults.string(forKey: "socket")
        // 入力メソッドのプロセスの PATH には利用者のシェルの設定が入らないので、rinzed は絶対パスで決める。
        // defaults の rinzed、ビルドのときに Info.plist に書いた RinzedPath の順に見て、どちらも無ければ PATH から探す
        let configured = [
            defaults.string(forKey: "rinzed"),
            Bundle.main.object(forInfoDictionaryKey: "RinzedPath") as? String,
        ]
        let program = configured.compactMap { $0 }.first { !$0.isEmpty } ?? "rinzed"
        guard let handle = rinze_service_new(socket, program) else {
            fatalError("rinze: 既定のソケットのパスを決められない (defaults の socket で指定する)")
        }
        self.handle = handle
        debug = defaults.bool(forKey: "debug")
    }

    func poll() -> (RinzeStatus, String) {
        var message: UnsafePointer<CChar>?
        let status = rinze_service_poll(handle, &message)
        return (status, message.map { String(cString: $0) } ?? "")
    }
}
