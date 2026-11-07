//! Terminal User Interface (TUI) Dashboard for Viernes Voice Assistant.
//!
//! Provides an ergonomic, low-overhead interactive terminal interface
//! with real-time soundwave spectrum visualization, live transcription history,
//! Push-to-Talk emulation, and modular management tabs.

pub mod app;
pub mod event;
pub mod terminal;
pub mod views;
pub mod widgets;

pub use app::{ActiveTab, App, TuiApp, MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH};
pub use event::run_app;
pub use terminal::{restore_terminal, setup_terminal, TerminalGuard};
pub use widgets::{
    send_ipc_ptt, ConversationTurn, ConversationWidget, PttWidget, Speaker, SpectrumWidget,
    BLOCK_CHARS, DAEMON_PTT_SOCKET,
};

use viernes_core::config::ViernesConfig;

/// Entry point for the `viernes --tui` command.
///
/// Configures terminal alternate screen, executes the non-blocking Tokio
/// event loop at 30 FPS, and restores terminal settings upon exit or panic.
pub async fn run_tui(config: ViernesConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard::new();

    let mut app = TuiApp::new(Some(config));

    // Initial log messages in service tab
    app.service_view
        .logs
        .push_line("Viernes Terminal Dashboard inicializado.");
    app.service_view
        .logs
        .push_line("Presione [1-5] para cambiar de pestaña o [q] para salir.");

    event::run_app(&mut terminal, app).await?;

    restore_terminal()?;
    Ok(())
}
