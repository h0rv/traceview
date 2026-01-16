//! TUI components for the tv binary.

mod app;
pub mod components;
mod state;
#[cfg(test)]
mod tests;

pub use app::App;
pub use state::{AppState, Focus};
