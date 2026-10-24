//! Rebuilds a pane's current screen as bytes for a fresh terminal emulator of
//! the pane's exact size. The pane modes a capture does not carry come from
//! format variables, as tmux integrations such as iTerm2's read them.

use std::collections::HashMap;
use std::fmt::Write;

use crate::client::Client;
use crate::escape::{decode_output, quote};
use crate::parser::Reply;
use crate::{Error, PaneId, WindowId};

/// A pane's screen. Feed `data` to a new emulator of `cols` x `rows`, then
/// every `%output` for the pane with `after_block >= block`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub pane: PaneId,
    pub window: WindowId,
    pub cols: u16,
    pub rows: u16,
    pub data: Vec<u8>,
    pub block: u64,
}

const SEP: char = '\u{1f}';

const STATE_FIELDS: [&str; 22] = [
    "window_id",
    "pane_width",
    "pane_height",
    "cursor_x",
    "cursor_y",
    "cursor_flag",
    "alternate_on",
    "alternate_saved_x",
    "alternate_saved_y",
    "scroll_region_upper",
    "scroll_region_lower",
    "keypad_cursor_flag",
    "keypad_flag",
    "insert_flag",
    "origin_flag",
    "wrap_flag",
    "bracket_paste_flag",
    "mouse_standard_flag",
    "mouse_button_flag",
    "mouse_any_flag",
    "mouse_sgr_flag",
    "mouse_utf8_flag",
];

const COMMANDS: usize = 4;

/// One command line, run atomically by tmux: pane state, the visible screen,
/// the primary screen (while the alternate one is shown), and an escape
/// sequence the program has started but not finished.
pub(crate) fn snapshot_line(pane: PaneId) -> String {
    let format: Vec<String> = STATE_FIELDS.iter().map(|f| format!("#{{{f}}}")).collect();
    let filter = quote(&format!("#{{==:#{{pane_id}},{pane}}}"));
    [
        format!(
            "list-panes -t {pane} -f {filter} -F {}",
            quote(&format.join(&SEP.to_string()))
        ),
        format!("capture-pane -p -e -N -t {pane}"),
        format!("capture-pane -p -e -N -a -q -t {pane}"),
        format!("capture-pane -p -P -C -t {pane}"),
    ]
    .join(" ; ")
}

pub async fn snapshot(client: &Client, pane: PaneId) -> Result<Screen, Error> {
    let results = client.run(&snapshot_line(pane), COMMANDS).await?;
    screen_from(pane, results)
}

pub(crate) fn screen_from(
    pane: PaneId,
    results: Vec<Result<Reply, Error>>,
) -> Result<Screen, Error> {
    let mut replies = Vec::with_capacity(COMMANDS);
    for r in results {
        replies.push(r.map_err(|e| e.for_pane(pane))?);
    }
    let [state, screen, primary, pending] = <[Reply; COMMANDS]>::try_from(replies)
        .map_err(|_| Error::Unexpected("snapshot reply count".into()))?;
    let line = state.lines.first().ok_or(Error::NoPane(pane))?;
    let st = State::parse(&String::from_utf8_lossy(line))?;
    // The last command's block: later output was not part of the capture.
    let block = pending.block;
    let pending = decode_output(&pending.lines.concat());
    Ok(Screen {
        pane,
        window: st.window,
        cols: st.get("pane_width") as u16,
        rows: st.get("pane_height") as u16,
        data: replay(&st, &screen.lines, &primary.lines, &pending),
        block,
    })
}

struct State {
    window: WindowId,
    values: HashMap<&'static str, u32>,
}

impl State {
    fn parse(line: &str) -> Result<Self, Error> {
        let fields: Vec<&str> = line.split(SEP).collect();
        if fields.len() != STATE_FIELDS.len() {
            return Err(Error::Unexpected(format!(
                "pane state has {} fields",
                fields.len()
            )));
        }
        let window = fields[0].parse()?;
        let values = STATE_FIELDS
            .iter()
            .zip(&fields)
            .skip(1)
            .map(|(k, v)| (*k, v.parse().unwrap_or(0)))
            .collect();
        Ok(Self { window, values })
    }

    fn get(&self, name: &str) -> u32 {
        self.values.get(name).copied().unwrap_or(0)
    }

    fn on(&self, name: &str) -> bool {
        self.get(name) == 1
    }
}

/// Escape sequences that put a reset emulator into the captured state.
fn replay(st: &State, screen: &[Vec<u8>], primary: &[Vec<u8>], pending: &[u8]) -> Vec<u8> {
    let rows = st.get("pane_height");
    let mut out = Vec::new();
    out.extend_from_slice(b"\x1bc"); // full reset
    if st.on("alternate_on") {
        write_rows(&mut out, primary, rows);
        cup(
            &mut out,
            st.get("alternate_saved_y"),
            st.get("alternate_saved_x"),
        );
        out.extend_from_slice(b"\x1b[?1049h"); // saves that cursor, switches screen, clears it
    }
    write_rows(&mut out, screen, rows);

    let (upper, lower) = (st.get("scroll_region_upper"), st.get("scroll_region_lower"));
    if upper != 0 || lower + 1 != rows {
        push(&mut out, format_args!("\x1b[{};{}r", upper + 1, lower + 1));
    }
    let modes: [(bool, &[u8]); 10] = [
        (st.on("keypad_cursor_flag"), b"\x1b[?1h"),
        (st.on("keypad_flag"), b"\x1b="),
        (st.on("insert_flag"), b"\x1b[4h"),
        (st.get("wrap_flag") == 0, b"\x1b[?7l"),
        (st.on("bracket_paste_flag"), b"\x1b[?2004h"),
        (st.on("mouse_standard_flag"), b"\x1b[?1000h"),
        (st.on("mouse_button_flag"), b"\x1b[?1002h"),
        (st.on("mouse_any_flag"), b"\x1b[?1003h"),
        (st.on("mouse_utf8_flag"), b"\x1b[?1005h"),
        (st.on("mouse_sgr_flag"), b"\x1b[?1006h"),
    ];
    for (on, seq) in modes {
        if on {
            out.extend_from_slice(seq);
        }
    }
    let mut y = st.get("cursor_y");
    if st.on("origin_flag") {
        out.extend_from_slice(b"\x1b[?6h"); // cursor addresses become relative to the region
        y = y.saturating_sub(upper);
    }
    cup(&mut out, y, st.get("cursor_x"));
    if st.get("cursor_flag") == 0 {
        out.extend_from_slice(b"\x1b[?25l");
    }
    out.extend_from_slice(pending);
    out
}

/// Places each row with an absolute cursor move, so the result does not
/// depend on line wrapping.
fn write_rows(out: &mut Vec<u8>, rows: &[Vec<u8>], height: u32) {
    for (i, row) in rows.iter().take(height as usize).enumerate() {
        push(out, format_args!("\x1b[0m\x1b[{};1H", i + 1));
        out.extend_from_slice(row);
    }
    out.extend_from_slice(b"\x1b[0m");
}

fn cup(out: &mut Vec<u8>, y: u32, x: u32) {
    push(out, format_args!("\x1b[{};{}H", y + 1, x + 1));
}

fn push(out: &mut Vec<u8>, args: std::fmt::Arguments<'_>) {
    let mut s = String::new();
    // Infallible: writing to a String.
    let _ = s.write_fmt(args);
    out.extend_from_slice(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_line(overrides: &[(&str, &str)]) -> Vec<u8> {
        STATE_FIELDS
            .iter()
            .map(|f| {
                overrides
                    .iter()
                    .find(|(k, _)| k == f)
                    .map(|(_, v)| *v)
                    .unwrap_or(match *f {
                        "window_id" => "@1",
                        "pane_width" => "10",
                        "pane_height" => "3",
                        "scroll_region_lower" => "2",
                        "cursor_flag" | "wrap_flag" => "1",
                        _ => "0",
                    })
            })
            .collect::<Vec<_>>()
            .join(&SEP.to_string())
            .into_bytes()
    }

    fn reply(lines: &[&[u8]], block: u64) -> Result<Reply, Error> {
        Ok(Reply {
            lines: lines.iter().map(|l| l.to_vec()).collect(),
            failed: false,
            block,
        })
    }

    #[test]
    fn plain_screen() {
        let st = state_line(&[("cursor_x", "2"), ("cursor_y", "1")]);
        let screen = screen_from(
            PaneId::new(5),
            vec![
                reply(&[&st], 7),
                reply(&[b"ab", b"\x1b[31mc", b""], 8),
                reply(&[], 9),
                reply(&[], 10),
            ],
        )
        .unwrap();
        assert_eq!((screen.cols, screen.rows, screen.block), (10, 3, 10));
        assert_eq!(screen.window, WindowId::new(1));
        assert_eq!(
            screen.data,
            b"\x1bc\x1b[0m\x1b[1;1Hab\x1b[0m\x1b[2;1H\x1b[31mc\x1b[0m\x1b[3;1H\x1b[0m\x1b[2;3H"
                .to_vec()
        );
    }

    #[test]
    fn alternate_screen_modes_region_and_pending_bytes() {
        let st = state_line(&[
            ("alternate_on", "1"),
            ("alternate_saved_x", "4"),
            ("alternate_saved_y", "2"),
            ("scroll_region_upper", "1"),
            ("cursor_flag", "0"),
            ("bracket_paste_flag", "1"),
            ("keypad_cursor_flag", "1"),
        ]);
        let screen = screen_from(
            PaneId::new(5),
            vec![
                reply(&[&st], 1),
                reply(&[b"alt"], 2),
                reply(&[b"main"], 3),
                reply(&[br"\033]0;ti"], 4),
            ],
        )
        .unwrap();
        let text = String::from_utf8(screen.data).unwrap();
        let primary = text.find("main").unwrap();
        let switch = text.find("\x1b[3;5H\x1b[?1049h").unwrap();
        let alt = text.find("alt").unwrap();
        assert!(primary < switch && switch < alt, "{text:?}");
        assert!(text.contains("\x1b[2;3r"));
        assert!(text.contains("\x1b[?1h") && text.contains("\x1b[?2004h"));
        assert!(text.ends_with("\x1b[1;1H\x1b[?25l\x1b]0;ti"), "{text:?}");
    }

    #[test]
    fn a_missing_pane_is_no_pane() {
        let gone = Err(Error::Tmux {
            message: "can't find pane: %5".into(),
        });
        let skipped = || Err(Error::Skipped { block: 1 });
        let err =
            screen_from(PaneId::new(5), vec![gone, skipped(), skipped(), skipped()]).unwrap_err();
        assert!(matches!(err, Error::NoPane(p) if p == PaneId::new(5)));
    }

    #[test]
    fn the_command_line_targets_by_id_and_quotes_the_format() {
        let line = snapshot_line(PaneId::new(3));
        assert_eq!(line.matches(" ; ").count(), COMMANDS - 1);
        assert!(line.contains("-t %3") && !line.contains('\n'));
    }
}
