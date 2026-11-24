// メニューバーに出す入力モードのアイコンを作る。
//   swift macos/icons.swift macos/Resources
// 作った TIFF はリポジトリに入れてあるので、字や大きさを変えるときだけ走らせる
import Cocoa

let glyphs = [
    ("hiragana", "あ"),
    ("katakana", "ア"),
    ("halfwidth-katakana", "ｱ"),
    ("fullwidth-latin", "Ａ"),
    ("ascii", "A"),
]
let points: CGFloat = 16
let directory = URL(fileURLWithPath: CommandLine.arguments[1])

for (name, glyph) in glyphs {
    let reps = [1, 2].map { scale -> NSBitmapImageRep in
        let pixels = Int(points) * scale
        let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0)!
        rep.size = NSSize(width: points, height: points)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        let text = NSAttributedString(
            string: glyph,
            attributes: [
                .font: NSFont(name: "HiraginoSans-W6", size: 13)!,
                .foregroundColor: NSColor.black,
            ])
        let size = text.size()
        text.draw(at: NSPoint(x: (points - size.width) / 2, y: (points - size.height) / 2))
        NSGraphicsContext.restoreGraphicsState()
        return rep
    }
    let data = NSBitmapImageRep.representationOfImageReps(in: reps, using: .tiff, properties: [:])!
    try data.write(to: directory.appendingPathComponent("\(name).tiff"))
}
