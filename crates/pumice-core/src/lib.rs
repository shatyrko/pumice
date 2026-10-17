//! Pumice engine. Everything here is plain Rust with no UI: the Tauri shell
//! only calls these functions and forwards their results.

pub mod apps;
pub mod clean;
pub mod duplicates;
pub mod history;
pub mod maintenance;
pub mod purge;
pub mod rules;
pub mod safety;
pub mod scan;
pub mod settings;
pub mod startup;
pub mod sysinfo;
pub mod util;

/// Result type used across the crate; errors are shown to the person as text.
pub type Result<T> = anyhow::Result<T>;
