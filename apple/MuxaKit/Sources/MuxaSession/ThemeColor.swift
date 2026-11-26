import Foundation

/// The color math of ADR 0091. Mixing is integer math, so Swift and Kotlin
/// give the same bytes; the shared fixture checks it.
extension RGB {
    public static let black = RGB(0, 0, 0)

    /// Moves each channel toward `other` by `percent` (0–100), rounded half up.
    public func mixed(toward other: RGB, _ percent: Int) -> RGB {
        let p = min(max(percent, 0), 100)
        func channel(_ a: UInt8, _ b: UInt8) -> UInt8 {
            UInt8((Int(a) * (100 - p) + Int(b) * p + 50) / 100)
        }
        return RGB(channel(r, other.r), channel(g, other.g), channel(b, other.b))
    }

    /// WCAG 2 relative luminance, from 0 (black) to 1 (white).
    public var luminance: Double {
        0.2126 * Self.linear[Int(r)] + 0.7152 * Self.linear[Int(g)] + 0.0722 * Self.linear[Int(b)]
    }

    /// WCAG 2 contrast ratio, from 1 to 21.
    public func contrast(with other: RGB) -> Double {
        let (a, b) = (luminance, other.luminance)
        return (max(a, b) + 0.05) / (min(a, b) + 0.05)
    }

    private static let linear: [Double] = (0...255).map { byte in
        let c = Double(byte) / 255
        return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
    }
}
