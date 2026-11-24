/// tmux key names, as `KeyMap` produces them (`C-c`, `M-x`, `S-Up`, `F5`,
/// `BSpace`), turned into the bytes an xterm sends. Protocol v2 carries bytes;
/// the key names stay the app's vocabulary for keys.
public enum KeyBytes {
    /// A message typed into an agent and sent with Return. Several lines go
    /// as one bracketed paste, as the server types a comment (ADR 0072), so
    /// the agent does not send each line on its own.
    public static func message(_ text: String) -> String {
        guard text.contains("\n") else { return text + "\r" }
        return "\u{1b}[200~" + text + "\u{1b}[201~\r"
    }

    /// The bytes for one key name; nil for a name no terminal sends.
    public static func bytes(_ name: String) -> [UInt8]? {
        var rest = Substring(name)
        var ctrl = false, alt = false, shift = false
        while rest.count > 2, let dash = rest.dropFirst().first, dash == "-" {
            switch rest.first {
            case "C": ctrl = true
            case "M": alt = true
            case "S": shift = true
            default: break
            }
            guard "CMS".contains(rest.first ?? " ") else { break }
            rest = rest.dropFirst(2)
        }
        let mod = 1 + (shift ? 1 : 0) + (alt ? 2 : 0) + (ctrl ? 4 : 0)
        if let seq = special(String(rest), mod: mod) { return seq }
        // One character after the modifiers.
        guard rest.count == 1 || rest == "Space", var c = rest == "Space" ? 0x20 : rest.utf8.first else { return nil }
        if ctrl {
            switch c {
            case 0x20, 0x40: c = 0
            case 0x3f: c = 0x7f
            case 0x61...0x7a, 0x41...0x5f: c &= 0x1f
            default: return nil
            }
        }
        return (alt ? [0x1b] : []) + [c]
    }

    private static func special(_ key: String, mod: Int) -> [UInt8]? {
        let esc = "\u{1b}"
        func csi(_ final: String, _ n: String = "1") -> String {
            mod == 1 ? "\(esc)[\(n == "1" ? "" : n)\(final)" : "\(esc)[\(n);\(mod)\(final)"
        }
        func tilde(_ n: Int) -> String { mod == 1 ? "\(esc)[\(n)~" : "\(esc)[\(n);\(mod)~" }
        func ss3(_ final: String) -> String { mod == 1 ? "\(esc)O\(final)" : "\(esc)[1;\(mod)\(final)" }
        let seq: String
        switch key {
        case "Up": seq = csi("A")
        case "Down": seq = csi("B")
        case "Right": seq = csi("C")
        case "Left": seq = csi("D")
        case "Home": seq = csi("H")
        case "End": seq = csi("F")
        case "PageUp": seq = tilde(5)
        case "PageDown": seq = tilde(6)
        case "IC": seq = tilde(2)
        case "DC": seq = tilde(3)
        case "Enter": seq = (mod - 1) & 2 != 0 ? "\(esc)\r" : "\r"
        case "Escape": seq = esc
        case "Tab": seq = "\t"
        case "BTab": seq = "\(esc)[Z"
        case "BSpace": seq = (mod - 1) & 2 != 0 ? "\(esc)\u{7f}" : "\u{7f}"
        case "F1": seq = ss3("P")
        case "F2": seq = ss3("Q")
        case "F3": seq = ss3("R")
        case "F4": seq = ss3("S")
        default:
            let fn = ["F5": 15, "F6": 17, "F7": 18, "F8": 19, "F9": 20, "F10": 21, "F11": 23, "F12": 24]
            guard let n = fn[key] else { return nil }
            seq = tilde(n)
        }
        return Array(seq.utf8)
    }
}
