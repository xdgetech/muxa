//! The muxa server's client sessions over protocol v2 (ADR 0067,
//! `protocol/PROTOCOL-v2.md`). It owns no tmux code; terminals come from a
//! [`muxa_backend::SessionBackend`]. It never logs terminal bytes.

mod attachment;
mod inbox;
pub mod memory_settings;
mod migrate;
pub mod reader;
mod ring;
mod session;
mod sessions;
mod shells;
mod task_actions;
mod task_items;
pub mod tasks;
mod uploads;
mod usage;
mod wire;

pub use muxa_inbox::Inbox;
pub use reader::ReaderOpen;
pub use session::{Config, PARK, ServeError, Services, serve};
pub use sessions::Sessions;
pub use task_items::watch_tasks;
pub use tasks::{Host, Prepared, RoutineList, TaskError, TaskSpec};
