pub mod account_storage;
pub mod commands;
pub mod config;
pub mod context;
pub mod errors;
pub mod event_listener;
pub mod rpc;
pub mod syscall_stubs;
pub mod types;

pub use config::Config;
pub use context::Context;
pub use errors::NeonCliError;

pub type NeonCliResult = Result<serde_json::Value, NeonCliError>;
