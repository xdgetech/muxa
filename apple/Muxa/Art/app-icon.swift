// Renders the app icon's light, dark and tinted images into AppIcon.appiconset.
// Run: swift apple/Muxa/Art/app-icon.swift <appiconset dir>
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

/// The prompt: a chevron and a cursor bar, centred on the canvas by their joint bounds.
func glyph() -> (chevron: CGPath, cursor: CGPath) {
    let stroke = 82.0
    let chevron = CGMutablePath()
    chevron.move(to: CGPoint(x: 0, y: 0))
    chevron.addLine(to: CGPoint(x: 190, y: 150))
    chevron.addLine(to: CGPoint(x: 0, y: 300))
    let chevronStroke = chevron.copy(strokingWithWidth: stroke, lineCap: .round, lineJoin: .round, miterLimit: 10)
    let cursor = CGPath(roundedRect: CGRect(x: 262, y: -stroke / 2, width: 210, height: stroke),
                        cornerWidth: stroke / 2, cornerHeight: stroke / 2, transform: nil)

    let bounds = chevronStroke.boundingBox.union(cursor.boundingBox)
    // Optical centre: the chevron's point carries the weight, so sit a little right of true centre.
    var move = CGAffineTransform(translationX: (size - bounds.width) / 2 - bounds.minX + 8,
                                 y: (size - bounds.height) / 2 - bounds.minY)
    return (chevronStroke.copy(using: &move)!, cursor.copy(using: &move)!)
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

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write(Data("usage: app-icon.swift <appiconset dir>\n".utf8))
    exit(2)
}
let dir = URL(fileURLWithPath: CommandLine.arguments[1])
do {
    for v in Variant.allCases {
        try write(render(v), to: dir.appendingPathComponent("icon-1024-\(v.rawValue).png"))
    }
} catch {
    FileHandle.standardError.write(Data("app-icon: \(error)\n".utf8))
    exit(1)
}
