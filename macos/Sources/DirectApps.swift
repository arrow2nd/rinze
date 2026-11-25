import Foundation

/// 直接入力にするアプリ。defaults の `directApps` にバンドル ID を並べて覚え、入力メソッドを起動し直しても残す。
struct DirectApps {
    static let key = "directApps"

    let defaults: UserDefaults

    func contains(_ app: String?) -> Bool {
        app.map { list.contains($0) } ?? false
    }

    /// `app` を直接入力にするかを入れ替え、入れ替えたあとに直接入力なら true を返す。
    @discardableResult
    func toggle(_ app: String) -> Bool {
        var apps = list
        if let index = apps.firstIndex(of: app) {
            apps.remove(at: index)
        } else {
            apps.append(app)
        }
        defaults.set(apps, forKey: Self.key)
        return apps.contains(app)
    }

    private var list: [String] {
        defaults.stringArray(forKey: Self.key) ?? []
    }
}
