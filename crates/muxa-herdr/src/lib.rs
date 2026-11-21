//! herdr-core client: JSON-RPC calls, attach streams and events over the
//! framed protocol of `hc connect` (ADR 0067, docs/findings/session-backends.md).
//! It never logs terminal bytes; only ids, sizes and errors.

mod backend;
mod client;
mod error;
mod events;
mod frame;
mod stream;
mod terminal;

pub use backend::{HerdrBackend, HerdrChanges, HerdrStream};
pub use client::{Client, PROTOCOL_GENERATION};
pub use error::Error;
pub use events::{Event, EventFilter, Events};
pub use frame::{Frame, FrameKind, MAX_PAYLOAD};
pub use stream::{Attach, AttachSpec, StreamEvent};
pub use terminal::{Name, RunSpec, Status, Terminal};
