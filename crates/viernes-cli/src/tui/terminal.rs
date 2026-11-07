//! Terminal lifecycle management, alternate screen setup, raw mode, and panic hooks.
//!
//! Enforces clean terminal restoration even in the event of unexpected panics.

use std::io::{self, stdout, Stdout};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

static TERMINAL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initializes the terminal into alternate screen with raw mode and cursor hidden.
/// Also installs a panic hook to safely restore the terminal if a panic occurs.
pub fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    install_panic_hook();

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
    TERMINAL_INITIALIZED.store(true, Ordering::SeqCst);

    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// Restores the terminal back to original cooked mode and shows the cursor.
pub fn restore_terminal() -> io::Result<()> {
    if TERMINAL_INITIALIZED.swap(false, Ordering::SeqCst) {
        disable_raw_mode()?;
        execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture, Show)?;
    }
    Ok(())
}

/// Installs a panic hook that safely resets the terminal before formatting the panic message.
fn install_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Intentionally ignore errors during panic terminal reset
        let _ = restore_terminal();
        original_hook(panic_info);
    }));
}

/// RAII Guard that ensures [`restore_terminal`] is called upon dropping.
#[derive(Debug, Default)]
pub struct TerminalGuard;

impl TerminalGuard {
    /// Creates a new guard that will restore the terminal when dropped.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_guard_creation() {
        let guard = TerminalGuard::new();
        // Dropping guard shouldn't panic even if terminal wasn't initialized in raw mode
        drop(guard);
    }
}
