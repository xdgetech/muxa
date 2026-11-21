//! The muxa server's client sessions over protocol v2 (ADR 0067,
//! `protocol/PROTOCOL-v2.md`). It owns no tmux code; terminals come from a
//! [`muxa_backend::SessionBackend`]. It never logs terminal bytes.

mod attachment;
mod ring;
mod session;
mod sessions;
pub mod tasks;
mod wire;

pub use session::{Config, PARK, ServeError, serve};
pub use sessions::Sessions;
pub use tasks::{Prepared, TaskError, TaskSpec, Tasks};
