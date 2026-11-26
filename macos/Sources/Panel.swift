import Cocoa

struct PanelLine {
    let text: String
    let note: String?
    let secondary: Bool

    init(_ text: String, note: String? = nil, secondary: Bool = false) {
        self.text = text
        self.note = note
        self.secondary = secondary
    }
}

/// 候補一覧、辞書登録の入力欄、注釈、知らせを出す小さな窓。入力欄のカーソルの下に置く。
final class Panel {
    static let shared = Panel()

    private let window: NSPanel
    private let label = NSTextField(wrappingLabelWithString: "")
    private weak var owner: AnyObject?
    private var lastCursor: NSRect?

    private init() {
        window = NSPanel(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
            defer: true)
        window.level = .popUpMenu
        window.hasShadow = true
        window.isOpaque = false
        window.backgroundColor = .clear
        // フルスクリーンのアプリとほかのスペースでも、入力欄の上に出す
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]

        let background = NSVisualEffectView()
        background.material = .popover
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 6
        label.font = .systemFont(ofSize: 14)
        label.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: background.leadingAnchor, constant: 8),
            label.trailingAnchor.constraint(equalTo: background.trailingAnchor, constant: -8),
            label.topAnchor.constraint(equalTo: background.topAnchor, constant: 6),
            label.bottomAnchor.constraint(equalTo: background.bottomAnchor, constant: -6),
        ])
        window.contentView = background
    }

    /// `cursor` が nil のとき (アプリがカーソルの位置を返さない) は、前に出した位置かマウスの位置に出す。
    func show(lines: [PanelLine], below cursor: NSRect?, owner: AnyObject) {
        guard !lines.isEmpty else {
            hide(owner: owner)
            return
        }
        self.owner = owner
        let cursor = cursor ?? lastCursor ?? NSRect(origin: NSEvent.mouseLocation, size: .zero)
        lastCursor = cursor
        let font = NSFont.systemFont(ofSize: 14)
        let primaryAttrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor.labelColor,
        ]
        let secondaryAttrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor.secondaryLabelColor,
        ]
        let attributed = NSMutableAttributedString()
        for (index, line) in lines.enumerated() {
            if index > 0 {
                attributed.append(NSAttributedString(string: "\n", attributes: primaryAttrs))
            }
            let baseAttrs = line.secondary ? secondaryAttrs : primaryAttrs
            attributed.append(NSAttributedString(string: line.text, attributes: baseAttrs))
            if let note = line.note, !note.isEmpty {
                attributed.append(
                    NSAttributedString(string: "  \(note)", attributes: secondaryAttrs))
            }
        }
        label.attributedStringValue = attributed
        label.preferredMaxLayoutWidth = 480
        let text = label.intrinsicContentSize
        let size = NSSize(width: text.width + 16, height: text.height + 12)
        var origin = NSPoint(x: cursor.minX, y: cursor.minY - size.height - 4)
        if let screen = NSScreen.screens.first(where: { $0.frame.contains(cursor.origin) }) {
            let visible = screen.visibleFrame
            if origin.y < visible.minY {
                origin.y = cursor.maxY + 4
            }
            origin.x = min(origin.x, visible.maxX - size.width)
        }
        window.setFrame(NSRect(origin: origin, size: size), display: true)
        window.orderFront(nil)
    }

    /// パネルは入力欄どうしで共有する。ほかの入力欄が出したものは消さない
    func hide(owner: AnyObject) {
        guard self.owner === owner else {
            return
        }
        self.owner = nil
        lastCursor = nil
        window.orderOut(nil)
    }
}
