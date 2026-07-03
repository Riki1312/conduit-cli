//! Library entry point for the `conduit` binary.
//!
//! The crate intentionally exposes only the CLI runner. The command
//! implementations, plugin runtime, and provider models are internal details
//! and should evolve behind the binary contract until a stable library API is
//! needed.

#![deny(missing_docs)]

mod app;

mod config;
mod db;
mod db_provider;
mod git_status;
mod logs;
mod logs_provider;
mod openapi;
mod openapi_provider;
mod output;
mod plugin_bindings;
mod plugin_check;
mod plugin_host;
mod plugin_runtime;
mod secrets;
mod state;
mod stats;
mod test_log;
mod test_report;
mod test_rerun;
mod test_run;
mod worktree;

pub use app::{CliError, run};
