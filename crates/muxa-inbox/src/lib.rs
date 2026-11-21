//! The Inbox of ADR 0066: things the user has to do, in the user's order.
//! Capture, Tasks and Routines add items; nothing reorders them but the user.

mod book;
mod capture;
mod service;
mod store;

pub use book::{Book, InboxError, NewItem};
pub use capture::parse;
pub use service::Inbox;
pub use store::StoreError;
