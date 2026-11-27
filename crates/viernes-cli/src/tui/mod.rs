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
    ptt_socket_path, send_ipc_ptt, ConversationTurn, ConversationWidget, PttWidget, Speaker,
    SpectrumWidget, BLOCK_CHARS,
};

use viernes_core::config::ViernesConfig;

/// Ensures the parent directory of `file_path` exists and is safe (`0o700` on Unix via
/// `viernes_core::ensure_private_dir`, without attempting to `chmod` the shared system temp root).
pub(crate) fn ensure_parent_dir(file_path: &std::path::Path) -> Result<(), String> {
    let Some(parent) = file_path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    if parent == std::env::temp_dir() {
        let meta = std::fs::symlink_metadata(parent)
            .map_err(|e| format!("Failed to inspect temp directory: {e}"))?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(format!(
                "Refusing to use invalid temp parent directory: {}",
                parent.display()
            ));
        }
        return Ok(());
    }
    viernes_core::ensure_private_dir(parent)
        .map_err(|e| format!("Failed to prepare config directory: {e}"))
}

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
