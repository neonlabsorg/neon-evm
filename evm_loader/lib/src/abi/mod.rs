mod cancel_trx;
mod collect_treasury;
mod context;
mod create_ether_account;
mod deposit;
mod emulate;
mod get_ether_account_data;
mod get_neon_elf;
mod get_storage_at;
mod init_environment;
mod trace;

use self::context::AbiContext;
use crate::{
    commands::get_neon_elf::CachedElfParams,
    config::{self},
    types::request_models::{EmulationParamsRequestModel, RequestWithSlot},
    Config, Context, LibMethods, NeonError,
};
use abi_stable::{
    prefix_type::WithMetadata,
    sabi_extern_fn,
    std_types::{RStr, RString},
};
use async_ffi::FutureExt;
use evm_loader::types::Address;
use lazy_static::lazy_static;
use neon_lib_interface::{
    types::{NeonEVMLibError, RNeonEVMLibResult},
    NeonEVMLib,
};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub const _MODULE_WM_: &WithMetadata<NeonEVMLib> = &WithMetadata::new(NeonEVMLib {
    hash,
    get_version,
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
fn invoke<'a>(method: RStr<'a>, params: RStr<'a>) -> RNeonEVMLibResult<'a> {
    async move {
        dispatch(method.as_str(), params.as_str())
            .await
            .map(RString::from)
            .map_err(neon_error_to_rstring)
            .into()
    }
    .into_local_ffi()
}

lazy_static! {
    static ref ABI_CONTEXT: AbiContext = build_context().unwrap();
}

fn build_context() -> Result<AbiContext, NeonError> {
    let api_options = config::load_api_config_from_enviroment();
    let config = config::create_from_api_config(&api_options)?;

    context::AbiContext::new(config)
}

async fn dispatch(method_str: &str, params_str: &str) -> Result<String, NeonError> {
    let method: LibMethods = method_str.parse()?;
    let RequestWithSlot {
        slot,
        tx_index_in_block,
    } = serde_json::from_str(params_str).map_err(|_| params_to_neon_error(params_str))?;
    let rpc_client = context::build_rpc_client(&ABI_CONTEXT, slot, tx_index_in_block).await?;
    let config = &ABI_CONTEXT.config;
    let context = crate::Context::new(rpc_client.as_ref(), config);

    match method {
        LibMethods::CreateEtherAccount => {
            create_ether_account::execute(&context, config, params_str)
                .await
                .map(|v| serde_json::to_string(&v).unwrap())
        }
        LibMethods::CancelTrx => cancel_trx::execute(&context, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethods::CollectTreasury => collect_treasury::execute(&context, config)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethods::Deposit => deposit::execute(&context, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethods::Emulate => emulate::execute(&context, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethods::GetEtherAccountData => {
            get_ether_account_data::execute(&context, config, params_str)
                .await
                .map(|v| serde_json::to_string(&v).unwrap())
        }
        LibMethods::GetNeonElf => get_neon_elf::execute(&context, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethods::GetStorageAt => get_storage_at::execute(&context, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethods::Trace => trace::execute(&context, config, params_str)
            .await
            .map(|v| serde_json::to_string(&v).unwrap()),
        LibMethods::InitEnvironment => init_environment::execute(&context, config, params_str)
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

pub async fn parse_emulation_params(
    config: &Config,
    context: &Context<'_>,
    params: &EmulationParamsRequestModel,
) -> (Pubkey, u64, u64, Vec<Address>, Vec<Pubkey>) {
    // Read ELF params only if token_mint or chain_id is not set.
    let mut token: Option<Pubkey> = params.token_mint.map(Into::into);
    let mut chain = params.chain_id;
    if token.is_none() || chain.is_none() {
        let cached_elf_params = CachedElfParams::new(config, context).await;
        token = token.or_else(|| {
            Some(
                Pubkey::from_str(
                    cached_elf_params
                        .get("NEON_TOKEN_MINT")
                        .expect("NEON_TOKEN_MINT load error"),
                )
                .expect("NEON_TOKEN_MINT Pubkey ctor error "),
            )
        });
        chain = chain.or_else(|| {
            Some(
                u64::from_str(
                    cached_elf_params
                        .get("NEON_CHAIN_ID")
                        .expect("NEON_CHAIN_ID load error"),
                )
                .expect("NEON_CHAIN_ID u64 ctor error"),
            )
        });
    }
    let token = token.expect("token_mint get error");
    let chain = chain.expect("chain_id get error");
    let max_steps = params.max_steps_to_execute;

    let accounts = params.cached_accounts.clone().unwrap_or_default();

    let solana_accounts = params
        .solana_accounts
        .clone()
        .map(|vec| vec.into_iter().map(Into::into).collect())
        .unwrap_or_default();

    (token, chain, max_steps, accounts, solana_accounts)
}
