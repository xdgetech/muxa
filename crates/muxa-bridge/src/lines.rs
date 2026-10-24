//! Reads request lines with a length cap. An over-long line is skipped to
//! its end and reported, so one bad request does not end the session.

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

pub enum Line {
    Text(Vec<u8>),
    TooLong,
}

/// The next line without its newline, or `None` at end of input.
pub async fn next<R: AsyncBufRead + Unpin>(r: &mut R, max: usize) -> std::io::Result<Option<Line>> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let buf = r.fill_buf().await?;
        if buf.is_empty() {
            return Ok(match (too_long, line.is_empty()) {
                (true, _) => Some(Line::TooLong),
                (false, true) => None,
                (false, false) => Some(Line::Text(line)),
            });
        }
        let (chunk, done) = match buf.iter().position(|b| *b == b'\n') {
            Some(i) => (i, true),
            None => (buf.len(), false),
        };
        if !too_long {
            line.extend_from_slice(&buf[..chunk]);
            if line.len() > max {
                too_long = true;
                line = Vec::new();
            }
        }
        r.consume(if done { chunk + 1 } else { chunk });
        if done {
            return Ok(Some(if too_long {
                Line::TooLong
            } else {
                Line::Text(line)
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn skips_an_over_long_line_and_continues() {
        let mut r = BufReader::with_capacity(3, &b"ok\n0123456789\nnext"[..]);
        assert!(matches!(next(&mut r, 5).await.unwrap(), Some(Line::Text(l)) if l == b"ok"));
        assert!(matches!(
            next(&mut r, 5).await.unwrap(),
            Some(Line::TooLong)
        ));
        assert!(matches!(next(&mut r, 5).await.unwrap(), Some(Line::Text(l)) if l == b"next"));
        assert!(next(&mut r, 5).await.unwrap().is_none());
    }
}
