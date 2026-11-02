//! tmux control-mode client: discovery, pane snapshots, output stream, input.
//! It never resizes a window or pane directly. Only `sizing` gives a client a
//! size, for the device in use, through a device session (ADR 0020, 0021).

mod client;
mod device;
mod discover;
mod error;
mod escape;
mod ids;
mod input;
mod lifecycle;
mod parser;
mod pending;
mod roles;
mod sizing;
mod snapshot;

pub use client::{Client, Flags, MAX_LINE, Notifications, Options};
pub use device::{
    DEVICE_TAG, Device, OWNER_TAG, devices_of, leave_device_session, unattached_devices,
};
pub use discover::{AgentState, AgentStatus, Pane, Role, Session, list_sessions, locate, pane};
pub use error::Error;
pub use escape::{decode_output, quote};
pub use ids::{PaneId, SessionId, WindowId};
pub use input::{paste, send_bytes, send_keys, send_text, valid_key};
pub use lifecycle::{
    clients_of, global_option, kill_session, last_other_session, new_session, pane_start_command,
    respawn_pane, session_names, set_pane_option, set_session_option, switch_client,
};
pub use parser::{Notification, Reply};
pub use sizing::{Grid, follows_device, release_size, resize_client, take_size, window_size};
pub use snapshot::{Screen, snapshot};
