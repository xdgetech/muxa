//! The session backend: what the muxa server needs from a terminal server,
//! one terminal at a time (ADR 0067). Workspaces, Tasks and pane layout are
//! not here; a backend only stores labels.

#[cfg(feature = "contract")]
pub mod contract;
mod error;
#[cfg(feature = "fake")]
pub mod fake;
mod types;

use std::future::Future;

pub use error::BackendError;
pub use types::{
    Change, Exit, Labels, Launch, ProgramStatus, Size, StreamEvent, TerminalId, TerminalInfo,
};

pub type Result<T> = std::result::Result<T, BackendError>;

/// A terminal server such as herdr-core.
pub trait SessionBackend: Send + Sync + 'static {
    type Stream: TerminalStream;
    type Changes: ChangeStream;

    /// Live terminals.
    fn list(&self) -> impl Future<Output = Result<Vec<TerminalInfo>>> + Send;

    /// One terminal, live or finished.
    fn get(&self, id: &TerminalId) -> impl Future<Output = Result<TerminalInfo>> + Send;

    /// Starts a program in a new terminal.
    fn run(&self, launch: &Launch) -> impl Future<Output = Result<TerminalInfo>> + Send;

    /// Opens a stream: a first paint with `history` lines of scrollback, then
    /// raw output. The viewer's size becomes the terminal's size while it is
    /// the last active viewer.
    fn attach(
        &self,
        id: &TerminalId,
        size: Size,
        history: u32,
    ) -> impl Future<Output = Result<Self::Stream>> + Send;

    /// The screen plus `lines` of history, as text.
    fn read(&self, id: &TerminalId, lines: u32) -> impl Future<Output = Result<String>> + Send;

    /// Ends the program. The terminal stays until [`SessionBackend::remove`].
    fn kill(&self, id: &TerminalId) -> impl Future<Output = Result<()>> + Send;

    /// Removes a finished terminal.
    fn remove(&self, id: &TerminalId) -> impl Future<Output = Result<()>> + Send;

    /// The terminals carrying all of `labels`, then every change to them.
    fn subscribe(
        &self,
        labels: &Labels,
    ) -> impl Future<Output = Result<(Vec<TerminalInfo>, Self::Changes)>> + Send;
}

/// An attached terminal.
pub trait TerminalStream: Send + 'static {
    /// The next event, or `None` after [`StreamEvent::Closed`].
    fn next(&mut self) -> impl Future<Output = Option<StreamEvent>> + Send;
    /// Bytes as if typed.
    fn input(&self, bytes: &[u8]) -> Result<()>;
    fn resize(&self, size: Size) -> Result<()>;
    /// Ends the stream; the program keeps running.
    fn detach(&self) -> Result<()>;
}

/// Changes to subscribed terminals, in order.
pub trait ChangeStream: Send + 'static {
    /// The next change, or `None` when the subscription ends.
    fn next(&mut self) -> impl Future<Output = Option<Result<Change>>> + Send;
}
