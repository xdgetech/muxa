//! Turns control-mode output lines into command replies and notifications.
//! Pure: the client feeds it lines; tests feed it recorded bytes.

use crate::PaneId;
use crate::escape::decode_output;

/// One complete `%begin` ... `%end`/`%error` block that answers a command this
/// client wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub lines: Vec<Vec<u8>>,
    pub failed: bool,
    /// Blocks completed since attach, this one included. A notification with
    /// `after_block >= block` was emitted after this command ran.
    pub block: u64,
}

/// A `%name args` line outside any block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub name: String,
    pub args: Vec<u8>,
    /// Blocks completed before this notification.
    pub after_block: u64,
}

impl Notification {
    /// The pane and decoded bytes of a `%output %<pane> <data>` notification.
    pub fn output(&self) -> Option<(PaneId, Vec<u8>)> {
        if self.name != "output" {
            return None;
        }
        let space = self.args.iter().position(|b| *b == b' ')?;
        let pane = std::str::from_utf8(&self.args[..space])
            .ok()?
            .parse()
            .ok()?;
        Some((pane, decode_output(&self.args[space + 1..])))
    }

    /// The first argument as text, e.g. the window id of `%layout-change`.
    pub fn first_arg(&self) -> &str {
        let end = self
            .args
            .iter()
            .position(|b| *b == b' ')
            .unwrap_or(self.args.len());
        std::str::from_utf8(&self.args[..end]).unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Reply(Reply),
    Notification(Notification),
}

#[derive(Debug, Default)]
pub struct Parser {
    open: Option<OpenBlock>,
    blocks: u64,
}

#[derive(Debug)]
struct OpenBlock {
    number: Vec<u8>,
    ours: bool,
    lines: Vec<Vec<u8>>,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one line without its trailing newline.
    pub fn feed(&mut self, line: Vec<u8>) -> Option<Event> {
        if let Some(open) = &mut self.open {
            // Only the terminator with the block's own number ends it: a
            // captured screen row may itself start with "%end ".
            if let Some(failed) = terminator(&line, &open.number) {
                let open = self.open.take()?;
                self.blocks += 1;
                if !open.ours {
                    return None;
                }
                return Some(Event::Reply(Reply {
                    lines: open.lines,
                    failed,
                    block: self.blocks,
                }));
            }
            open.lines.push(line);
            return None;
        }
        if let Some(rest) = line.strip_prefix(b"%begin ") {
            let fields: Vec<&[u8]> = rest.split(|b| *b == b' ').collect();
            if let [_time, number, flags, ..] = fields[..] {
                self.open = Some(OpenBlock {
                    number: number.to_vec(),
                    // flags bit 0: the command came from this client. The
                    // attach command at startup has flags 0.
                    ours: std::str::from_utf8(flags)
                        .ok()
                        .and_then(|f| f.parse::<u32>().ok())
                        .is_some_and(|f| f & 1 == 1),
                    lines: Vec::new(),
                });
            }
            return None;
        }
        let rest = line.strip_prefix(b"%")?;
        let (name, args) = match rest.iter().position(|b| *b == b' ') {
            Some(i) => (&rest[..i], rest[i + 1..].to_vec()),
            None => (rest, Vec::new()),
        };
        Some(Event::Notification(Notification {
            name: String::from_utf8_lossy(name).into_owned(),
            args,
            after_block: self.blocks,
        }))
    }
}

/// `Some(failed)` when `line` is `%end`/`%error` for block `number`.
fn terminator(line: &[u8], number: &[u8]) -> Option<bool> {
    let (rest, failed) = match line.strip_prefix(b"%end ") {
        Some(r) => (r, false),
        None => (line.strip_prefix(b"%error ")?, true),
    };
    let mut fields = rest.split(|b| *b == b' ');
    let _time = fields.next();
    (fields.next() == Some(number)).then_some(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(p: &mut Parser, text: &[u8]) -> Vec<Event> {
        text.split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .filter_map(|l| p.feed(l.to_vec()))
            .collect()
    }

    #[test]
    fn ignores_the_startup_block_and_counts_it() {
        let mut p = Parser::new();
        let events = feed_all(
            &mut p,
            b"%begin 1 10 0\n%end 1 10 0\n%session-changed $0 a\n",
        );
        assert_eq!(
            events,
            vec![Event::Notification(Notification {
                name: "session-changed".into(),
                args: b"$0 a".to_vec(),
                after_block: 1
            })]
        );
    }

    #[test]
    fn returns_our_replies_with_their_lines() {
        let mut p = Parser::new();
        let events = feed_all(&mut p, b"%begin 1 11 1\none\ntwo\n%end 1 11 1\n");
        assert_eq!(
            events,
            vec![Event::Reply(Reply {
                lines: vec![b"one".to_vec(), b"two".to_vec()],
                failed: false,
                block: 1
            })]
        );
    }

    #[test]
    fn a_row_that_looks_like_a_terminator_stays_content() {
        let mut p = Parser::new();
        let events = feed_all(
            &mut p,
            b"%begin 1 12 1\n%end 9 99 1\n%error x\n%output %1 hi\n%end 1 12 1\n",
        );
        let Event::Reply(r) = &events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(r.lines.len(), 3);
        assert_eq!(r.lines[2], b"%output %1 hi");
    }

    #[test]
    fn marks_failed_blocks() {
        let mut p = Parser::new();
        let events = feed_all(
            &mut p,
            b"%begin 1 13 1\ncan't find pane: %9\n%error 1 13 1\n",
        );
        assert!(matches!(&events[0], Event::Reply(r) if r.failed));
    }

    #[test]
    fn tags_notifications_with_blocks_seen() {
        let mut p = Parser::new();
        let events = feed_all(
            &mut p,
            b"%output %1 a\n%begin 1 14 1\n%end 1 14 1\n%output %1 b\n%exit\n",
        );
        let after: Vec<(String, u64)> = events
            .iter()
            .filter_map(|e| match e {
                Event::Notification(n) => Some((n.name.clone(), n.after_block)),
                Event::Reply(_) => None,
            })
            .collect();
        assert_eq!(
            after,
            vec![
                ("output".into(), 0),
                ("output".into(), 1),
                ("exit".into(), 1)
            ]
        );
    }

    #[test]
    fn decodes_output_notifications() {
        let mut p = Parser::new();
        let events = feed_all(
            &mut p,
            b"%output %3 a\\015\\012\xff\n%layout-change @2 abc\n",
        );
        let Event::Notification(n) = &events[0] else {
            panic!()
        };
        assert_eq!(n.output(), Some((PaneId::new(3), b"a\r\n\xff".to_vec())));
        let Event::Notification(n) = &events[1] else {
            panic!()
        };
        assert_eq!(n.output(), None);
        assert_eq!(n.first_arg(), "@2");
    }

    #[test]
    fn keeps_raw_bytes_in_notifications() {
        let mut p = Parser::new();
        let events = feed_all(&mut p, b"%output %1 \xff\xfe\n");
        assert!(matches!(&events[0], Event::Notification(n) if n.args == b"%1 \xff\xfe"));
    }
}
