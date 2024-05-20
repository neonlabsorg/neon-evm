#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::future_not_send,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::module_name_repetitions
)]

pub mod abi;
pub mod account_data;
pub mod account_storage;
pub mod build_info;
pub mod build_info_common;
pub mod commands;
pub mod config;
pub mod errors;
pub mod rpc;

pub mod solana_simulator;
pub mod tracing;
pub mod types;

use abi::_MODULE_WM_;
use abi_stable::export_root_module;
pub use config::Config;
pub use errors::NeonError;
use neon_lib_interface::NeonEVMLib_Ref;

pub type NeonResult<T> = Result<T, NeonError>;

const MODULE: NeonEVMLib_Ref = NeonEVMLib_Ref(_MODULE_WM_.static_as_prefix());

#[export_root_module]
#[must_use]
pub const fn get_root_module() -> NeonEVMLib_Ref {
    MODULE
}

use strum_macros::{AsRefStr, Display, EnumString, IntoStaticStr};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Display, EnumString, IntoStaticStr, AsRefStr)]
pub enum LibMethod {
    #[strum(serialize = "emulate")]
    Emulate,
    #[strum(serialize = "get_storage_at")]
    GetStorageAt,
    #[strum(serialize = "config")]
    GetConfig,
    #[strum(serialize = "balance")]
    GetBalance,
    #[strum(serialize = "contract")]
    GetContract,
    #[strum(serialize = "holder")]
    GetHolder,
    #[strum(serialize = "trace")]
    Trace,
    #[strum(serialize = "simulate_solana")]
    SimulateSolana,
}
