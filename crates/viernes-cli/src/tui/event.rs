//! Non-blocking Tokio event loop and Crossterm asynchronous event handling.

use std::io;
use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, EventStream};
use ratatui::backend::Backend;
use ratatui::Terminal;
use tokio_stream::StreamExt;

use crate::tui::app::TuiApp;

/// Frequency in milliseconds for the animation and UI refresh tick (~30 FPS).
pub const TICK_RATE_MS: u64 = 33;

/// Runs the main asynchronous event and rendering loop for the TUI dashboard.
pub async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: TuiApp) -> io::Result<()> {
    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(TICK_RATE_MS));

    while !app.should_quit {
        // Render current application state
        terminal.draw(|f| app.render(f))?;

        tokio::select! {
            // Periodic animation tick (30 FPS)
            _ = ticker.tick() => {
                app.on_tick();
            }

            // Crossterm asynchronous stream event
            maybe_event = reader.next() => {
                if let Some(Ok(event)) = maybe_event {
                    match event {
                        CrosstermEvent::Key(key) => {
                            app.handle_key(key).await;
                        }
                        CrosstermEvent::Resize(_, _) => {
                            // Automatic re-render on next cycle
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}
