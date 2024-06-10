mod emulate;
mod get_balance;
mod get_config;
mod get_contract;
mod get_holder;
mod get_storage_at;
mod simulate_solana;
pub mod state;
mod trace;

use crate::{
    abi::state::State,
    config::{self, APIOptions},
    types::RequestWithSlot,
    LibMethod, NeonError,
};
use abi_stable::{
    prefix_type::WithMetadata,
    sabi_extern_fn,
    std_types::{RStr, RString},
};
use async_ffi::FutureExt;
use lazy_static::lazy_static;
use neon_lib_interface::{
    types::{NeonEVMLibError, RNeonEVMLibResult},
    NeonEVMLib,
};
use serde_json::json;

lazy_static! {
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
    static ref STATE: State = state_sync();
}

pub fn state_sync() -> State {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        State::new(load_config()).await
    })
}

pub const _MODULE_WM_: &WithMetadata<NeonEVMLib> = &WithMetadata::new(NeonEVMLib {
    hash,
    get_version,
    get_build_info,
    invoke,
});

#[sabi_extern_fn]
fn hash() -> RString {
    env!("NEON_REVISION").into()
}

#[sabi_extern_fn]
fn get_version() -> RString {
    env!("CARGO_PKG_VERSION").into()
}

#[sabi_extern_fn]
fn get_build_info() -> RString {
    json!(crate::build_info::get_build_info())
        .to_string()
        .into()
}

#[sabi_extern_fn]
fn invoke<'a>(method: RStr<'a>, params: RStr<'a>) -> RNeonEVMLibResult<'a> {
    async move {
        // Needed for tokio::task::spawn_blocking using thread local storage inside dynamic library
        // since dynamic library and executable have different thread local storage namespaces
        let _guard = RUNTIME.enter();

        dispatch(method.as_str(), params.as_str())
            .await
            .map(RString::from)
            .map_err(neon_error_to_rstring)
            .into()
    }
    .into_local_ffi()
}

fn load_config() -> APIOptions {
    config::load_api_config_from_environment()
}

async fn dispatch(method_str: &str, params_str: &str) -> Result<String, NeonError> {
    let method: LibMethod = method_str.parse()?;
    let RequestWithSlot {
        slot,
        tx_index_in_block,
    } = match params_str {
        "" => RequestWithSlot {
            slot: None,
            tx_index_in_block: None,
        },
        _ => serde_json::from_str(params_str).map_err(|_| params_to_neon_error(params_str))?,
    };
    let state = &STATE;
    let config = &state.config;
    let rpc = state.build_rpc(slot, tx_index_in_block).await?;

    match method {
        LibMethod::Emulate => emulate::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetStorageAt => get_storage_at::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetBalance => get_balance::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetConfig => get_config::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetContract => get_contract::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetHolder => get_holder::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::Trace => trace::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::SimulateSolana => simulate_solana::execute(&rpc, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        // _ => Err(NeonError::IncorrectLibMethod),
    }
}

fn params_to_neon_error(params: &str) -> NeonError {
    NeonError::EnvironmentError(
        crate::commands::init_environment::EnvironmentError::InvalidProgramParameter(params.into()),
    )
}

fn neon_error_to_neon_lib_error(error: &NeonError) -> NeonEVMLibError {
    assert!(error.error_code() != 0);
    NeonEVMLibError {
        code: error.error_code(),
        message: error.to_string(),
        data: None,
    }
}

#[allow(clippy::needless_pass_by_value)]
fn neon_error_to_rstring(error: NeonError) -> RString {
    RString::from(serde_json::to_string(&neon_error_to_neon_lib_error(&error)).unwrap())
}
