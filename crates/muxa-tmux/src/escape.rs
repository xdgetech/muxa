//! The two text boundaries with tmux: decoding `%output` payloads, and
//! quoting values placed in a command line. Every command line muxa builds
//! puts untrusted values through [`quote`].

use std::fmt::Write;

/// Undoes `%output` escaping: tmux writes bytes below 0x20 and the backslash
/// as `\ooo` octal. Every other byte, including invalid UTF-8, is raw.
pub fn decode_output(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        // The first digit is at most 3: a byte is at most \377.
        if s[i] == b'\\'
            && i + 3 < s.len()
            && (b'0'..=b'3').contains(&s[i + 1])
            && s[i + 2..=i + 3].iter().all(|b| (b'0'..=b'7').contains(b))
        {
            let n = (s[i + 1] - b'0') << 6 | (s[i + 2] - b'0') << 3 | (s[i + 3] - b'0');
            out.push(n);
            i += 4;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// Makes `s` one double-quoted tmux command argument. `"`, `\` and `$` are
/// escaped, and control bytes become `\ooo`, so the value can neither end the
/// argument or the line nor expand a variable.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' | '\\' | '$' => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                // Infallible: writing to a String.
                let _ = write!(out, "\\{:03o}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_octal_escapes_and_keeps_raw_bytes() {
        assert_eq!(decode_output(br"a\015\012b"), b"a\r\nb");
        assert_eq!(decode_output(br"\134"), b"\\");
        assert_eq!(decode_output(b"\xff\xfe"), b"\xff\xfe");
        assert_eq!(decode_output("é".as_bytes()), "é".as_bytes());
    }

    #[test]
    fn leaves_incomplete_escapes_alone() {
        assert_eq!(decode_output(br"\01"), br"\01");
        assert_eq!(decode_output(br"\9"), br"\9");
        assert_eq!(decode_output(br"x\"), br"x\");
        assert_eq!(decode_output(br"\777"), br"\777");
    }

    #[test]
    fn quotes_every_special_character() {
        assert_eq!(quote(r#"a"b$c\d"#), r#""a\"b\$c\\d""#);
        assert_eq!(quote("\t\n\r\u{1b}\u{7f}"), r#""\011\012\015\033\177""#);
        assert_eq!(quote("é😀 ;{}#~"), "\"é😀 ;{}#~\"");
        assert_eq!(quote(""), "\"\"");
    }

    #[test]
    fn quoted_values_never_contain_a_line_break_or_bare_quote() {
        let all: String = (0u32..0x250).filter_map(char::from_u32).collect();
        let q = quote(&all);
        let inner = &q[1..q.len() - 1];
        assert!(!q.contains('\n') && !q.contains('\r'));
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => assert!(chars.next().is_some()),
                '"' | '$' => panic!("unescaped {c:?} in {q}"),
                _ => {}
            }
        }
    }
}
