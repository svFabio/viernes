//! Views and widgets for the Viernes Terminal Dashboard.

pub mod credentials;
pub mod devices;
pub mod fillers;
pub mod logs;
pub mod service;

pub use credentials::{CredentialsAction, CredentialsFocus, CredentialsView};
pub use devices::{DeviceFocus, DevicesView};
pub use fillers::{FillerEditorMode, FillersAction, FillersView};
pub use logs::{LogEntry, LogSeverity, LogsView, MAX_LOG_BUFFER_LINES};
pub use service::{DaemonHealth, ServiceControlRunner, ServiceView};
