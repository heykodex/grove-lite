//! Pure logic behind Grove Lite.
//!
//! Everything here is free of any UI or webview dependency so it can be unit
//! tested on any machine: turning typed text into a URL, deciding which
//! navigations are allowed, the tab list, keyboard actions, the local history
//! store, and safe download paths.

pub mod download;
pub mod history;
pub mod input;
pub mod shortcut;
pub mod tabs;

pub use history::{Entry, History};
pub use input::{display_url, is_allowed_navigation, resolve, search_url, Resolved};
pub use shortcut::Action;
pub use tabs::{CloseOutcome, Kind, Snapshot, Tab, TabView, Tabs};
