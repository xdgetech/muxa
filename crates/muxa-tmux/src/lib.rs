//! tmux control-mode client: discovery, pane snapshots, output stream, input.
//! It never resizes a window or pane; see `docs/adr/0001-control-mode-ignore-size.md`.

mod client;
mod discover;
mod error;
mod escape;
mod ids;
mod input;
mod lifecycle;
mod parser;
mod pending;
mod roles;
mod snapshot;

pub use client::{Client, MAX_LINE, Notifications, Options};
pub use discover::{AgentState, AgentStatus, Pane, Role, Session, list_sessions, locate, pane};
pub use error::Error;
pub use escape::{decode_output, quote};
pub use ids::{PaneId, SessionId, WindowId};
pub use input::{paste, send_bytes, send_keys, send_text, valid_key};
pub use lifecycle::{
    clients_of, global_option, kill_session, last_other_session, new_session, session_names,
    set_pane_option, set_session_option, switch_client,
};
pub use parser::{Notification, Reply};
pub use snapshot::{Screen, snapshot};
