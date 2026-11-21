//! The muxa server's client sessions over protocol v2 (ADR 0067,
//! `protocol/PROTOCOL-v2.md`). It owns no tmux code; terminals come from a
//! [`muxa_backend::SessionBackend`]. It never logs terminal bytes.

mod attachment;
mod session;
mod wire;

pub use session::{Config, ServeError, serve};
