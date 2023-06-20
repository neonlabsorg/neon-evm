mod account_storage;
pub mod commands;
pub mod config;
pub mod context;
mod errors;
mod event_listener;
pub mod logs;
pub mod program_options;
mod rpc;
mod syscall_stubs;
pub mod types;

pub use config::Config;
pub use context::Context;
pub use errors::NeonCliError;

pub type NeonCliResult = Result<serde_json::Value, NeonCliError>;
