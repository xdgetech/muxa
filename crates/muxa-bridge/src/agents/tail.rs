//! Follows a JSONL file that an agent appends to: the last part at first, then
//! each new complete line. Shared by the adapters that read transcript files.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::Error;

/// Where reading starts on open: at most this much before the end.
pub const INITIAL_TAIL: u64 = 1 << 20;
/// A longer line (a huge tool result, an image) is skipped, not buffered.
pub const MAX_LINE: usize = 8 << 20;
/// The most read in one poll, so one poll never stalls on a burst.
const MAX_READ: u64 = 4 << 20;

pub struct JsonlTail {
    path: PathBuf,
    ino: u64,
    offset: u64,
    partial: Vec<u8>,
    /// Inside a line longer than `MAX_LINE`; drop bytes until its newline.
    skipping: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Poll {
    Lines(Vec<Vec<u8>>),
    /// The file shrank or is another file now: open it again.
    Replaced,
}

impl JsonlTail {
    /// Opens `path` and returns the complete lines of its last `tail` bytes.
    /// `truncated` is true when the file has more before them.
    pub fn open(path: &Path, tail: u64) -> Result<(Self, Vec<Vec<u8>>, bool), Error> {
        let meta = std::fs::metadata(path).map_err(|e| io(path, e))?;
        let start = meta.len().saturating_sub(tail);
        let mut t = Self {
            path: path.to_owned(),
            ino: meta.ino(),
            offset: start,
            partial: Vec::new(),
            // Starting mid-file: the first line is probably cut, so drop it.
            skipping: start > 0,
        };
        let lines = match t.poll()? {
            Poll::Lines(lines) => lines,
            Poll::Replaced => Vec::new(),
        };
        Ok((t, lines, start > 0))
    }

    /// The complete lines added since the last call.
    pub fn poll(&mut self) -> Result<Poll, Error> {
        let meta = match std::fs::metadata(&self.path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Poll::Replaced),
            Err(e) => return Err(io(&self.path, e)),
        };
        if meta.ino() != self.ino || meta.len() < self.offset {
            return Ok(Poll::Replaced);
        }
        if meta.len() == self.offset {
            return Ok(Poll::Lines(Vec::new()));
        }
        let mut file = File::open(&self.path).map_err(|e| io(&self.path, e))?;
        file.seek(SeekFrom::Start(self.offset))
            .map_err(|e| io(&self.path, e))?;
        let want = (meta.len() - self.offset).min(MAX_READ);
        let mut buf = Vec::with_capacity(want as usize);
        file.take(want)
            .read_to_end(&mut buf)
            .map_err(|e| io(&self.path, e))?;
        self.offset += buf.len() as u64;
        Ok(Poll::Lines(self.split(&buf)))
    }

    fn split(&mut self, mut buf: &[u8]) -> Vec<Vec<u8>> {
        let mut lines = Vec::new();
        while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            let (head, rest) = buf.split_at(nl);
            buf = &rest[1..];
            if std::mem::take(&mut self.skipping) {
                self.partial.clear();
                continue;
            }
            if self.partial.len() + head.len() > MAX_LINE {
                tracing::warn!(
                    bytes = self.partial.len() + head.len(),
                    "skipping an over-long transcript line"
                );
                self.partial.clear();
                continue;
            }
            self.partial.extend_from_slice(head);
            let line = std::mem::take(&mut self.partial);
            if !line.iter().all(u8::is_ascii_whitespace) {
                lines.push(line);
            }
        }
        if !self.skipping {
            self.partial.extend_from_slice(buf);
            if self.partial.len() > MAX_LINE {
                tracing::warn!(
                    bytes = self.partial.len(),
                    "skipping an over-long transcript line"
                );
                self.partial.clear();
                self.skipping = true;
            }
        }
        lines
    }
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muxa-tail-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("t.jsonl")
    }

    fn append(path: &Path, text: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    fn texts(lines: Vec<Vec<u8>>) -> Vec<String> {
        lines
            .into_iter()
            .map(|l| String::from_utf8(l).unwrap())
            .collect()
    }

    #[test]
    fn returns_complete_lines_and_keeps_a_partial_one() {
        let path = temp("partial");
        std::fs::write(&path, "a\nb\nc").unwrap();
        let (mut t, lines, truncated) = JsonlTail::open(&path, INITIAL_TAIL).unwrap();
        assert_eq!(texts(lines), ["a", "b"]);
        assert!(!truncated);
        append(&path, "d\ne\n");
        assert_eq!(
            t.poll().unwrap(),
            Poll::Lines(vec![b"cd".to_vec(), b"e".to_vec()])
        );
        assert_eq!(t.poll().unwrap(), Poll::Lines(vec![]));
    }

    #[test]
    fn a_short_tail_drops_the_cut_first_line() {
        let path = temp("tail");
        std::fs::write(&path, "first line\nsecond\nthird\n").unwrap();
        let (_, lines, truncated) = JsonlTail::open(&path, 10).unwrap();
        assert_eq!(texts(lines), ["third"]);
        assert!(truncated);
    }

    #[test]
    fn a_rewritten_or_removed_file_is_reported() {
        let path = temp("replaced");
        std::fs::write(&path, "a\nb\n").unwrap();
        let (mut t, _, _) = JsonlTail::open(&path, INITIAL_TAIL).unwrap();
        std::fs::write(&path, "x\n").unwrap();
        assert_eq!(t.poll().unwrap(), Poll::Replaced);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(t.poll().unwrap(), Poll::Replaced);
    }

    #[test]
    fn an_over_long_line_is_skipped() {
        let path = temp("long");
        std::fs::write(&path, "ok\n").unwrap();
        let (mut t, _, _) = JsonlTail::open(&path, INITIAL_TAIL).unwrap();
        let mut huge = "x".repeat(MAX_LINE + 10);
        huge.push_str("\nafter\n");
        append(&path, &huge);
        let mut got = Vec::new();
        while let Poll::Lines(lines) = t.poll().unwrap() {
            if lines.is_empty() && t.offset == std::fs::metadata(&path).unwrap().len() {
                break;
            }
            got.extend(texts(lines));
        }
        assert_eq!(got, ["after"]);
    }
}
