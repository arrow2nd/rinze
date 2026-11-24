import Cocoa
import InputMethodKit

private let leftContextWindow = 80
private let startupPollInterval: TimeInterval = 0.1
private let modeFlashInterval: TimeInterval = 1

/// 入力欄 1 つぶんの rinzed の接続。表示は最後の応答をそのまま写し、入力の状態は rinzed だけが持つ。
@objc(RinzeInputController)
final class RinzeInputController: IMKInputController {
    private var connection: OpaquePointer?
    private var shown: OpaquePointer?
    private var notice = ""
    private var diagnosis = ""
    private var modeFlash = ""
    private var marked = false
    private var startupTimer: Timer?
    private var modeTimer: Timer?

    override init!(server: IMKServer!, delegate: Any!, client inputClient: Any!) {
        super.init(server: server, delegate: delegate, client: inputClient)
        connection = rinze_client_new(Service.shared.handle)
    }

    deinit {
        startupTimer?.invalidate()
        modeTimer?.invalidate()
        rinze_response_free(shown)
        rinze_client_free(connection)
    }

    private var mode: RinzeMode? { shown.map { rinze_response_mode($0) } }
    private var passthrough: Bool { shown.map { rinze_response_passthrough($0) } ?? false }
    private var idle: Bool { shown.map { rinze_response_idle($0) } ?? true }
    private var showsCandidates: Bool {
        shown.map { rinze_response_candidate_count($0) > 0 } ?? false
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event, event.type == .keyDown else {
            return false
        }
        var name = keyName(event)
        if passthrough && name != "ctrl-j" {
            return false
        }
        if showsCandidates {
            if event.keyCode == KeyCode.up {
                name = "x"
            } else if event.keyCode == KeyCode.down {
                name = "space"
            }
        }
        if name == nil {
            if idle {
                return false
            }
            if event.keyCode != KeyCode.escape {
                flush()
                return false
            }
            name = "ctrl-g"
        }
        return press(name!)
    }

    override func activateServer(_ sender: Any!) {
        if shown == nil {
            apply(rinze_client_discard(connection))
        } else {
            render()
        }
    }

    // フォーカスが外れたときは、入力途中のものを確定せずに捨てる。入力モードは残る
    override func deactivateServer(_ sender: Any!) {
        discard()
        Panel.shared.hide()
    }

    // 入力ソースを切り替えたときやアプリが確定を求めたときは、表示どおりに確定する
    override func commitComposition(_ sender: Any!) {
        flush()
    }

    private func press(_ key: String) -> Bool {
        let surrounding = Service.shared.debug || !passthrough ? leftSurrounding() : nil
        let response = rinze_client_key(
            connection, key, surrounding?.text, surrounding?.cursor ?? 0)
        if Service.shared.debug {
            var left: UnsafePointer<CChar>?
            let origin = rinze_client_left_context(connection, &left)
            let text = left.map { String(cString: $0) } ?? ""
            diagnosis =
                surrounding == nil ? "左文脈 (取れない)" : "左文脈 (\(originName(origin))): \(text)"
        }
        return apply(response)
    }

    private func flush() {
        if !idle {
            apply(rinze_client_flush(connection))
        }
    }

    private func discard() {
        if !idle {
            apply(rinze_client_discard(connection))
        }
    }

    /// カーソルより前の文字列。未確定文字列があれば、その前まで。
    private func leftSurrounding() -> (text: String, cursor: Int)? {
        guard let client = client() else {
            return nil
        }
        let markedRange = client.markedRange()
        let end =
            markedRange.location != NSNotFound
            ? markedRange.location : client.selectedRange().location
        guard end != NSNotFound else {
            return nil
        }
        let range = leftRange(end: end, window: leftContextWindow)
        if range.length == 0 {
            return ("", 0)
        }
        var actual = NSRange(location: NSNotFound, length: 0)
        guard let text = client.string(from: range, actualRange: &actual) else {
            return nil
        }
        return (text, text.unicodeScalars.count)
    }

    @discardableResult
    private func apply(_ response: OpaquePointer?) -> Bool {
        guard let response else {
            var message: UnsafePointer<CChar>?
            let failure = rinze_client_failure(connection, &message)
            notice = message.map { String(cString: $0) } ?? ""
            NSLog("rinze: %@", notice)
            if failure == RINZE_FAILURE_DELAYED {
                render()
                return true
            }
            rinze_response_free(shown)
            shown = nil
            render()
            if failure == RINZE_FAILURE_STARTING {
                watchStartup()
            }
            return false
        }
        let commit = String(cString: rinze_response_commit(response))
        if !commit.isEmpty {
            client()?.insertText(commit, replacementRange: NSRange(location: NSNotFound, length: 0))
            marked = false
        }
        let before = mode
        let handled = rinze_response_handled(response)
        rinze_response_free(shown)
        shown = response
        notice = ""
        if let before, let after = mode, before != after {
            flashMode(after)
        }
        render()
        return handled
    }

    private func render() {
        guard let client = client() else {
            return
        }
        let preedit = shown.map { String(cString: rinze_response_preedit($0)) } ?? ""
        if !preedit.isEmpty || marked {
            let text = NSAttributedString(
                string: preedit,
                attributes: [
                    .underlineStyle: NSUnderlineStyle.single.rawValue,
                    .markedClauseSegment: 0,
                ])
            client.setMarkedText(
                text, selectionRange: NSRange(location: preedit.utf16.count, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0))
        }
        marked = !preedit.isEmpty

        var lines: [String] = []
        if !notice.isEmpty {
            lines.append("rinze: \(notice)")
        }
        if let shown {
            var prompt: UnsafePointer<CChar>?
            var text: UnsafePointer<CChar>?
            var typing: UnsafePointer<CChar>?
            if rinze_response_minibuffer(shown, &prompt, &text, &typing),
                let prompt, let text, let typing
            {
                lines.append(
                    String(cString: prompt) + String(cString: text) + String(cString: typing))
            }
            let count = rinze_response_candidate_count(shown)
            for index in 0..<count {
                let label = String(cString: rinze_response_candidate_label(shown, index))
                let word = String(cString: rinze_response_candidate_word(shown, index))
                lines.append("\(label): \(word)")
            }
            if count > 0 {
                lines.append("[\(rinze_response_page(shown) + 1)/\(rinze_response_pages(shown))]")
            }
        }
        if lines.isEmpty && !modeFlash.isEmpty {
            lines.append(modeFlash)
        }
        if Service.shared.debug && !diagnosis.isEmpty {
            lines.append(diagnosis)
        }
        var cursor = NSRect.zero
        client.attributes(forCharacterIndex: 0, lineHeightRectangle: &cursor)
        Panel.shared.show(lines: lines, below: cursor)
    }

    private func flashMode(_ mode: RinzeMode) {
        modeFlash = modeLabel(mode)
        modeTimer?.invalidate()
        modeTimer = Timer.scheduledTimer(withTimeInterval: modeFlashInterval, repeats: false) {
            [weak self] _ in
            self?.modeFlash = ""
            self?.render()
        }
    }

    // rinzed の起動を待つ間、待ち受けを始めたか起動に失敗したかを確かめ、打鍵の無いうちに表示を直す
    private func watchStartup() {
        startupTimer?.invalidate()
        startupTimer = Timer.scheduledTimer(withTimeInterval: startupPollInterval, repeats: true) {
            [weak self] timer in
            guard let self else {
                timer.invalidate()
                return
            }
            let (status, message) = Service.shared.poll()
            if status == RINZE_STATUS_STARTING {
                return
            }
            timer.invalidate()
            if status == RINZE_STATUS_READY {
                self.apply(rinze_client_discard(self.connection))
            } else {
                self.notice = message
                self.render()
            }
        }
    }
}

private func originName(_ origin: RinzeLeftContext) -> String {
    switch origin {
    case RINZE_LEFT_CONTEXT_SURROUNDING: return "surrounding"
    case RINZE_LEFT_CONTEXT_PREEDIT_REMOVED: return "未確定文字列を除いた"
    default: return "なし"
    }
}
