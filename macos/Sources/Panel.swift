import Cocoa

/// 候補一覧、辞書登録の入力欄、知らせを出す小さな窓。入力欄のカーソルの下に置く。
final class Panel {
    static let shared = Panel()

    private let window: NSPanel
    private let label = NSTextField(wrappingLabelWithString: "")

    private init() {
        window = NSPanel(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
            defer: true)
        window.level = .popUpMenu
        window.hasShadow = true
        window.isOpaque = false
        window.backgroundColor = .clear

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

    func show(lines: [String], below cursor: NSRect) {
        guard !lines.isEmpty else {
            hide()
            return
        }
        label.stringValue = lines.joined(separator: "\n")
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

    func hide() {
        window.orderOut(nil)
    }
}
