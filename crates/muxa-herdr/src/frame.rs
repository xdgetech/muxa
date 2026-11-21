//! The framed mode of herdr-core: `[channel u32 LE][kind u8][len u32 LE][payload]`.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::Error;

/// herdr-core's limit on one frame's payload.
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;

const HEADER: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Channel 0: one JSON-RPC message. Other channels: stream bytes.
    Data,
    /// u32 LE: bytes the receiver consumed.
    Credit,
    /// The channel is over. From the core, a JSON reason.
    Close,
    /// u16 LE cols, u16 LE rows.
    Resize,
    Focus,
    Facts,
    AutomaticInput,
}

impl FrameKind {
    fn code(self) -> u8 {
        match self {
            FrameKind::Data => 0,
            FrameKind::Credit => 1,
            FrameKind::Close => 2,
            FrameKind::Resize => 3,
            FrameKind::Focus => 4,
            FrameKind::Facts => 5,
            FrameKind::AutomaticInput => 6,
        }
    }

    fn from_code(code: u8) -> Result<Self, Error> {
        Ok(match code {
            0 => FrameKind::Data,
            1 => FrameKind::Credit,
            2 => FrameKind::Close,
            3 => FrameKind::Resize,
            4 => FrameKind::Focus,
            5 => FrameKind::Facts,
            6 => FrameKind::AutomaticInput,
            other => return Err(Error::UnknownKind(other)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: u32,
    pub kind: FrameKind,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(channel: u32, kind: FrameKind, payload: Vec<u8>) -> Self {
        Frame {
            channel,
            kind,
            payload,
        }
    }

    pub fn credit(channel: u32, consumed: u32) -> Self {
        Frame::new(channel, FrameKind::Credit, consumed.to_le_bytes().to_vec())
    }

    pub fn resize(channel: u32, cols: u16, rows: u16) -> Self {
        let mut payload = cols.to_le_bytes().to_vec();
        payload.extend_from_slice(&rows.to_le_bytes());
        Frame::new(channel, FrameKind::Resize, payload)
    }

    /// The header and payload as one buffer.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let len = self.payload.len();
        if len > MAX_PAYLOAD {
            return Err(Error::FrameTooLarge(len));
        }
        let len32 = u32::try_from(len).map_err(|_| Error::FrameTooLarge(len))?;
        let mut out = Vec::with_capacity(HEADER + len);
        out.extend_from_slice(&self.channel.to_le_bytes());
        out.push(self.kind.code());
        out.extend_from_slice(&len32.to_le_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// Reads one frame. `Ok(None)` at a clean end of stream before a header.
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<Frame>, Error> {
        let mut header = [0u8; HEADER];
        match reader.read_exact(&mut header).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let channel = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let kind = FrameKind::from_code(header[4])?;
        let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
        if len > MAX_PAYLOAD {
            return Err(Error::FrameTooLarge(len));
        }
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await?;
        Ok(Some(Frame {
            channel,
            kind,
            payload,
        }))
    }

    pub async fn write<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&self.encode()?).await?;
        writer.flush().await?;
        Ok(())
    }
}

/// Parses a RESIZE payload.
pub(crate) fn resize_size(payload: &[u8]) -> Option<(u16, u16)> {
    match payload {
        [c0, c1, r0, r1] => Some((
            u16::from_le_bytes([*c0, *c1]),
            u16::from_le_bytes([*r0, *r1]),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_frame_round_trips() {
        let frame = Frame::new(7, FrameKind::Data, b"hello".to_vec());
        let bytes = frame.encode().unwrap();
        assert_eq!(&bytes[..9], &[7, 0, 0, 0, 0, 5, 0, 0, 0]);
        let mut reader = bytes.as_slice();
        assert_eq!(Frame::read(&mut reader).await.unwrap(), Some(frame));
        assert_eq!(Frame::read(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn resize_and_credit_payloads_are_little_endian() {
        let resize = Frame::resize(2, 120, 40);
        assert_eq!(resize.payload, vec![120, 0, 40, 0]);
        assert_eq!(resize_size(&resize.payload), Some((120, 40)));
        assert_eq!(Frame::credit(2, 300).payload, 300u32.to_le_bytes().to_vec());
    }

    #[tokio::test]
    async fn an_oversized_or_unknown_frame_is_an_error() {
        let mut header = vec![1, 0, 0, 0, 0];
        header.extend_from_slice(&((MAX_PAYLOAD as u32) + 1).to_le_bytes());
        let mut reader = header.as_slice();
        assert!(matches!(
            Frame::read(&mut reader).await,
            Err(Error::FrameTooLarge(_))
        ));
        let unknown = [1u8, 0, 0, 0, 9, 0, 0, 0, 0];
        let mut reader = &unknown[..];
        assert!(matches!(
            Frame::read(&mut reader).await,
            Err(Error::UnknownKind(9))
        ));
    }

    #[tokio::test]
    async fn a_cut_payload_is_an_io_error() {
        let mut bytes = Frame::new(1, FrameKind::Data, b"abcdef".to_vec())
            .encode()
            .unwrap();
        bytes.truncate(12);
        let mut reader = bytes.as_slice();
        assert!(matches!(Frame::read(&mut reader).await, Err(Error::Io(_))));
    }
}
