mod cancel_trx;
mod collect_treasury;
mod emulate;
mod get_balance;
mod get_config;
mod get_contract;
mod get_holder;
mod get_neon_elf;
mod get_storage_at;
mod init_environment;
mod trace;

use crate::{
    config::{self},
    rpc::{CallDbClient, RpcEnum},
    types::{RequestWithSlot, TracerDb},
    Config, LibMethod, NeonError,
};
use abi_stable::{
    prefix_type::WithMetadata,
    sabi_extern_fn,
    std_types::{RStr, RString},
};
use async_ffi::FutureExt;
use clap::ArgMatches;
use lazy_static::lazy_static;
use neon_lib_interface::{
    types::{NeonEVMLibError, RNeonEVMLibResult},
    NeonEVMLib,
};
use serde_json::json;
use solana_clap_utils::keypair::signer_from_path;
use solana_sdk::signer::Signer;

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

lazy_static! {
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
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

async fn load_config() -> Result<Config, NeonError> {
    let api_options = config::load_api_config_from_enviroment();
    let config = config::create_from_api_config(&api_options)?;

    Ok(config)
}

async fn dispatch(method_str: &str, params_str: &str) -> Result<String, NeonError> {
    let method: LibMethod = method_str.parse()?;
    let config = load_config().await?;
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
    let rpc = build_rpc(&config, slot, tx_index_in_block).await?;
    let singer = build_signer(&config)?;

    match method {
        LibMethod::CancelTrx => cancel_trx::execute(
            &config.build_solana_rpc_client(),
            &*singer,
            &config,
            params_str,
        )
        .await
        .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::CollectTreasury => collect_treasury::execute(
            &config.build_clone_solana_rpc_client(),
            &*singer,
            &config,
            params_str,
        )
        .await
        .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::Emulate => emulate::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetNeonElf => get_neon_elf::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetStorageAt => get_storage_at::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetBalance => get_balance::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetConfig => get_config::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetContract => get_contract::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::GetHolder => get_holder::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::Trace => trace::execute(&rpc, &config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethod::InitEnvironment => init_environment::execute(
            &config.build_clone_solana_rpc_client(),
            &*singer,
            &config,
            params_str,
        )
        .await
        .map(|v| serde_json::to_string(&v).unwrap()), // _ => Err(NeonError::IncorrectLibMethod),
    }
}

fn params_to_neon_error(params: &str) -> NeonError {
    NeonError::EnvironmentError(
        crate::commands::init_environment::EnvironmentError::InvalidProgramParameter(params.into()),
    )
}

fn neon_error_to_neon_lib_error(error: NeonError) -> NeonEVMLibError {
    assert!(error.error_code() >= 0);
    NeonEVMLibError {
        code: error.error_code() as u32,
        message: error.to_string(),
        data: None,
    }
}

fn neon_error_to_rstring(error: NeonError) -> RString {
    RString::from(serde_json::to_string(&neon_error_to_neon_lib_error(error)).unwrap())
}

fn build_signer(config: &Config) -> Result<Box<dyn Signer>, NeonError> {
    let mut wallet_manager = None;

    let signer = signer_from_path(
        &ArgMatches::default(),
        &config.keypair_path,
        "keypair",
        &mut wallet_manager,
    )
    .map_err(|_| NeonError::KeypairNotSpecified)?;

    Ok(signer)
}

async fn build_rpc(
    config: &Config,
    slot: Option<u64>,
    tx_index_in_block: Option<u64>,
) -> Result<RpcEnum, NeonError> {
    Ok(if let Some(slot) = slot {
        RpcEnum::CallDbClient(
            CallDbClient::new(
                TracerDb::new(config.db_config.as_ref().expect("db-config not found")),
                slot,
                tx_index_in_block,
            )
            .await?,
        )
    } else {
        RpcEnum::CloneRpcClient(config.build_clone_solana_rpc_client())
    })
}
