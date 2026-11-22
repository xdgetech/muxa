//! The local terminal in raw mode while `attach` shows a terminal.

use std::io::Write;

use crossterm::terminal;

/// Restores the local terminal however the view ends.
pub(crate) struct Raw;

impl Raw {
    pub(crate) fn enter() -> Result<Raw, String> {
        terminal::enable_raw_mode().map_err(|e| format!("cannot use this terminal: {e}"))?;
        print!("\x1b[?1049h");
        let _ = std::io::stdout().flush();
        Ok(Raw)
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        // Soft reset first: the program may have left modes on (mouse, keypad).
        print!("\x1b[!p\x1b[?1049l\x1b[?25h");
        let _ = std::io::stdout().flush();
        let _ = terminal::disable_raw_mode();
    }
}
