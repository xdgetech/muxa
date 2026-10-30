// Renders the app icon from one glyph: iOS's light, dark and tinted images, Android's
// adaptive icon layers, and Google Play's store icon and feature graphic.
// Run: swift apple/Muxa/Art/app-icon.swift <appiconset dir> <android res dir> <play listing dir>
import AppKit
import CoreGraphics

let size = 1024.0

/// The accent from AccentColor.colorset, light and dark.
let accentLight = rgb(0x5E, 0x5C, 0xE6)
let accentDark = rgb(0x7D, 0x7A, 0xFF)

enum Variant: String, CaseIterable {
    case light, dark, tinted
}

func rgb(_ r: Int, _ g: Int, _ b: Int, _ a: Double = 1) -> CGColor {
    CGColor(srgbRed: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255, alpha: a)
}

let stroke = 82.0

/// The prompt's centre line and cursor bar on the 1024 canvas, y up, before stroking.
func glyphGeometry() -> (chevron: [CGPoint], cursor: CGRect) {
    let chevron = [CGPoint(x: 0, y: 0), CGPoint(x: 190, y: 150), CGPoint(x: 0, y: 300)]
    let cursor = CGRect(x: 262, y: -stroke / 2, width: 210, height: stroke)
    let line = CGMutablePath()
    line.addLines(between: chevron)
    let bounds = line.copy(strokingWithWidth: stroke, lineCap: .round, lineJoin: .round, miterLimit: 10).boundingBox
        .union(cursor)
    // Optical centre: the chevron's point carries the weight, so sit a little right of true centre.
    let move = CGAffineTransform(translationX: (size - bounds.width) / 2 - bounds.minX + 8,
                                 y: (size - bounds.height) / 2 - bounds.minY)
    return (chevron.map { $0.applying(move) }, cursor.applying(move))
}

/// The prompt: a chevron and a cursor bar, centred on the canvas by their joint bounds.
func glyph() -> (chevron: CGPath, cursor: CGPath) {
    let (points, bar) = glyphGeometry()
    let chevron = CGMutablePath()
    chevron.addLines(between: points)
    let cursor = CGPath(roundedRect: bar, cornerWidth: stroke / 2, cornerHeight: stroke / 2, transform: nil)
    return (chevron.copy(strokingWithWidth: stroke, lineCap: .round, lineJoin: .round, miterLimit: 10), cursor)
}

func render(_ variant: Variant) -> CGImage {
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8, bytesPerRow: 0,
                        space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let (chevron, cursor) = glyph()

    switch variant {
    case .light:
        // CoreGraphics' y axis points up: start top-left, end bottom-right.
        let bg = CGGradient(colorsSpace: space, colors: [rgb(0x86, 0x83, 0xFF), accentLight, rgb(0x3F, 0x37, 0xC2)] as CFArray,
                            locations: [0, 0.45, 1])!
        ctx.drawLinearGradient(bg, start: CGPoint(x: 0, y: size), end: CGPoint(x: size, y: 0), options: [])
        // A soft light from above, so the flat field reads as a surface.
        let glow = CGGradient(colorsSpace: space, colors: [rgb(255, 255, 255, 0.22), rgb(255, 255, 255, 0)] as CFArray,
                              locations: [0, 1])!
        ctx.drawRadialGradient(glow, startCenter: CGPoint(x: size * 0.3, y: size * 1.05), startRadius: 0,
                               endCenter: CGPoint(x: size * 0.3, y: size * 1.05), endRadius: size * 0.85, options: [])
        ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: rgb(0x1E, 0x18, 0x7A, 0.45))
        ctx.setFillColor(rgb(255, 255, 255))
        ctx.addPath(chevron)
        ctx.addPath(cursor)
        ctx.fillPath()
    case .dark:
        // iOS draws its own dark backdrop behind a transparent dark icon.
        fill(ctx, [chevron, cursor], with: CGGradient(colorsSpace: space, colors: [rgb(0xA9, 0xA7, 0xFF), accentDark] as CFArray,
                                                      locations: [0, 1])!)
    case .tinted:
        // iOS tints by luminance: white is full tint.
        fill(ctx, [chevron, cursor], with: CGGradient(colorsSpace: space, colors: [rgb(255, 255, 255), rgb(0xD8, 0xD8, 0xD8)] as CFArray,
                                                      locations: [0, 1])!)
    }
    return ctx.makeImage()!
}

/// Fills the paths with a top-to-bottom gradient.
func fill(_ ctx: CGContext, _ paths: [CGPath], with gradient: CGGradient) {
    ctx.saveGState()
    for p in paths { ctx.addPath(p) }
    ctx.clip()
    ctx.drawLinearGradient(gradient, start: CGPoint(x: 0, y: size * 0.7), end: CGPoint(x: 0, y: size * 0.3),
                           options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
    ctx.restoreGState()
}

func write(_ image: CGImage, to url: URL) throws {
    let rep = NSBitmapImageRep(cgImage: image)
    guard let png = rep.representation(using: .png, properties: [:]) else {
        throw CocoaError(.fileWriteUnknown)
    }
    try png.write(to: url)
}

// MARK: Android

/// An adaptive icon is 108 dp, and its safe zone a 66 dp circle. The iOS canvas maps to
/// the middle 72 dp, so the glyph keeps its share of the icon and stays inside the zone.
let androidViewport = size * 108 / 72
let androidInset = (androidViewport - size) / 2

/// A point on the iOS canvas (y up) in the Android viewport (y down).
func android(_ p: CGPoint) -> String {
    String(format: "%.1f,%.1f", p.x + androidInset, size - p.y + androidInset)
}

func hex(_ r: Int, _ g: Int, _ b: Int, alpha: Double = 1) -> String {
    String(format: "#%02X%02X%02X%02X", Int((alpha * 255).rounded()), r, g, b)
}

func vector(_ body: String) -> String {
    """
    <?xml version="1.0" encoding="utf-8"?>
    <!-- Generated by apple/Muxa/Art/app-icon.swift (make app-icon). Do not edit. -->
    <vector xmlns:android="http://schemas.android.com/apk/res/android"
        xmlns:aapt="http://schemas.android.com/aapt"
        android:width="108dp"
        android:height="108dp"
        android:viewportWidth="\(androidViewport)"
        android:viewportHeight="\(androidViewport)">
    \(body)
    </vector>

    """
}

/// The glyph in white. The launcher tints the same layer for themed icons.
func androidForeground() -> String {
    let (points, bar) = glyphGeometry()
    let r = stroke / 2
    // CG's minY is the bar's bottom edge; Android's y runs down, so it becomes the larger y.
    let pill = "M\(android(CGPoint(x: bar.minX + r, y: bar.maxY))) " +
        String(format: "H%.1f ", bar.maxX - r + androidInset) +
        "A\(r),\(r) 0 0 1 \(android(CGPoint(x: bar.maxX - r, y: bar.minY))) " +
        String(format: "H%.1f ", bar.minX + r + androidInset) +
        "A\(r),\(r) 0 0 1 \(android(CGPoint(x: bar.minX + r, y: bar.maxY))) Z"
    return vector("""
        <path
            android:strokeColor="#FFFFFFFF"
            android:strokeWidth="\(stroke)"
            android:strokeLineCap="round"
            android:strokeLineJoin="round"
            android:pathData="M\(android(points[0])) L\(android(points[1])) L\(android(points[2]))" />
        <path
            android:fillColor="#FFFFFFFF"
            android:pathData="\(pill)" />
    """)
}

/// The iOS light icon's gradient and its soft light from above.
func androidBackground() -> String {
    let full = "M0,0 H\(androidViewport) V\(androidViewport) H0 Z"
    let topLeft = android(CGPoint(x: 0, y: size)), bottomRight = android(CGPoint(x: size, y: 0))
    let start = topLeft.split(separator: ","), end = bottomRight.split(separator: ",")
    let glow = android(CGPoint(x: size * 0.3, y: size * 1.05)).split(separator: ",")
    return vector("""
        <path android:pathData="\(full)">
            <aapt:attr name="android:fillColor">
                <gradient
                    android:type="linear"
                    android:startX="\(start[0])"
                    android:startY="\(start[1])"
                    android:endX="\(end[0])"
                    android:endY="\(end[1])">
                    <item android:offset="0" android:color="\(hex(0x86, 0x83, 0xFF))" />
                    <item android:offset="0.45" android:color="\(hex(0x5E, 0x5C, 0xE6))" />
                    <item android:offset="1" android:color="\(hex(0x3F, 0x37, 0xC2))" />
                </gradient>
            </aapt:attr>
        </path>
        <path android:pathData="\(full)">
            <aapt:attr name="android:fillColor">
                <gradient
                    android:type="radial"
                    android:centerX="\(glow[0])"
                    android:centerY="\(glow[1])"
                    android:gradientRadius="\(size * 0.85)">
                    <item android:offset="0" android:color="\(hex(255, 255, 255, alpha: 0.22))" />
                    <item android:offset="1" android:color="\(hex(255, 255, 255, alpha: 0))" />
                </gradient>
            </aapt:attr>
        </path>
    """)
}

// MARK: Google Play

/// Play's store icon: the light icon, square. Play rounds the corners itself.
func playIcon() -> CGImage {
    let pixels = 512
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
                        space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
    ctx.interpolationQuality = .high
    ctx.draw(render(.light), in: CGRect(x: 0, y: 0, width: pixels, height: pixels))
    return ctx.makeImage()!
}

/// Play's 1024x500 feature graphic: the glyph and the name on the icon's gradient.
func featureGraphic(tagline: String) -> CGImage {
    let canvas = CGSize(width: 1024, height: 500)
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: Int(canvas.width), height: Int(canvas.height), bitsPerComponent: 8,
                        bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
    let bg = CGGradient(colorsSpace: space, colors: [rgb(0x86, 0x83, 0xFF), accentLight, rgb(0x3F, 0x37, 0xC2)] as CFArray,
                        locations: [0, 0.45, 1])!
    ctx.drawLinearGradient(bg, start: CGPoint(x: 0, y: canvas.height), end: CGPoint(x: canvas.width, y: 0), options: [])

    // The glyph alone, without the icon's own background, at the left.
    let (chevron, cursor) = glyph()
    ctx.saveGState()
    ctx.translateBy(x: 40, y: (canvas.height - 420) / 2)
    ctx.scaleBy(x: 420 / size, y: 420 / size)
    ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: rgb(0x1E, 0x18, 0x7A, 0.45))
    ctx.setFillColor(rgb(255, 255, 255))
    ctx.addPath(chevron)
    ctx.addPath(cursor)
    ctx.fillPath()
    ctx.restoreGState()

    NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
    let left = 470.0
    let name = NSAttributedString(string: "Muxa", attributes: [
        .font: NSFont.systemFont(ofSize: 120, weight: .bold), .foregroundColor: NSColor.white,
    ])
    name.draw(at: CGPoint(x: left, y: 250))
    let style = NSMutableParagraphStyle()
    style.lineBreakMode = .byWordWrapping
    let line = NSAttributedString(string: tagline, attributes: [
        .font: NSFont.systemFont(ofSize: 40, weight: .medium), .foregroundColor: NSColor(white: 1, alpha: 0.88),
        .paragraphStyle: style,
    ])
    line.draw(in: CGRect(x: left, y: 110, width: canvas.width - left - 50, height: 130))
    NSGraphicsContext.current = nil
    return ctx.makeImage()!
}

/// Per Play listing locale.
let taglines = [
    "en-US": "The AI agents in your tmux, on your phone and tablet.",
    "ja-JP": "tmuxのAIエージェントを、スマホとタブレットで。",
]

guard CommandLine.arguments.count == 4 else {
    FileHandle.standardError.write(Data("usage: app-icon.swift <appiconset dir> <android res dir> <play listing dir>\n".utf8))
    exit(2)
}
let dir = URL(fileURLWithPath: CommandLine.arguments[1])
let res = URL(fileURLWithPath: CommandLine.arguments[2])
let play = URL(fileURLWithPath: CommandLine.arguments[3])
do {
    for v in Variant.allCases {
        try write(render(v), to: dir.appendingPathComponent("icon-1024-\(v.rawValue).png"))
    }
    try androidForeground().write(to: res.appendingPathComponent("drawable/ic_launcher_foreground.xml"), atomically: true, encoding: .utf8)
    try androidBackground().write(to: res.appendingPathComponent("drawable/ic_launcher_background.xml"), atomically: true, encoding: .utf8)
    for (locale, tagline) in taglines {
        let images = play.appendingPathComponent("\(locale)/images")
        try FileManager.default.createDirectory(at: images, withIntermediateDirectories: true)
        try write(playIcon(), to: images.appendingPathComponent("icon.png"))
        try write(featureGraphic(tagline: tagline), to: images.appendingPathComponent("featureGraphic.png"))
    }
} catch {
    FileHandle.standardError.write(Data("app-icon: \(error)\n".utf8))
    exit(1)
}
