pub mod abi;
pub mod account_storage;
pub mod build_info;
pub mod build_info_common;
pub mod commands;
pub mod config;
pub mod context;
pub mod errors;
pub mod rpc;
pub mod syscall_stubs;
pub mod types;

use abi::_MODULE_WM_;
use abi_stable::export_root_module;
pub use config::Config;
pub use context::Context;
pub use errors::NeonError;
use neon_lib_interface::NeonEVMLib_Ref;

pub type NeonResult<T> = Result<T, NeonError>;

const MODULE: NeonEVMLib_Ref = NeonEVMLib_Ref(_MODULE_WM_.static_as_prefix());

#[export_root_module]
pub fn get_root_module() -> NeonEVMLib_Ref {
    MODULE
}

use strum_macros::{AsRefStr, Display, EnumString, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, Display, EnumString, IntoStaticStr, AsRefStr)]
pub enum LibMethods {
    #[strum(serialize = "emulate")]
    Emulate,
    #[strum(serialize = "get_ether_account_data")]
    GetEtherAccountData,
    #[strum(serialize = "get_storage_at")]
    GetStorageAt,
    #[strum(serialize = "trace")]
    Trace,
    #[strum(serialize = "cancel_trx")]
    CancelTrx,
    #[strum(serialize = "collect_treasury")]
    CollectTreasury,
    #[strum(serialize = "create_ether_account")]
    CreateEtherAccount,
    #[strum(serialize = "deposit")]
    Deposit,
    #[strum(serialize = "get_neon_elf")]
    GetNeonElf,
    #[strum(serialize = "init_environment")]
    InitEnvironment,
}
