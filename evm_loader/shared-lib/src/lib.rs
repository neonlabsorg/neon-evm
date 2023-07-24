use abi_stable::{
    export_root_module,
    prefix_type::WithMetadata,
    sabi_extern_fn,
    std_types::{RResult, RStr, RString},
    DynTrait,
};
use async_ffi::{BorrowingFfiFuture, FutureExt};
use neon_interface::{
    types::{BoxedConfig, BoxedContext, NeonLibError, RNeonResult},
    NeonLib, NeonLib_Ref,
};
use neon_lib::{
    commands::{
        cancel_trx, collect_treasury, create_ether_account, deposit, emulate,
        get_ether_account_data, get_neon_elf, get_storage_at, init_environment,
    },
    config::create_from_api_comnfig,
    context::{build_hash_rpc_client, build_rpc_client},
    signer::NeonSigner,
    Config, Context, NeonError,
};

const _MODULE_WM_: &WithMetadata<NeonLib> = &WithMetadata::new(NeonLib {
    hash,
    get_version,
    init_config,
    init_context,
    init_hash_context,
    invoke,
});

fn params_to_neon_error(params: &str) -> NeonError {
    NeonError::EnvironmentError(
        neon_lib::commands::init_environment::EnvironmentError::InvalidProgramParameter(
            params.into(),
        ),
    )
}

fn neon_error_to_neon_lib_error(error: NeonError) -> NeonLibError {
    NeonLibError {
        code: error.error_code() as u32,
        message: error.to_string(),
        data: None,
    }
}

fn neon_error_to_rstring(error: NeonError) -> RString {
    RString::from(serde_json::to_string(&neon_error_to_neon_lib_error(error)).unwrap())
}

const MODULE: NeonLib_Ref = NeonLib_Ref(_MODULE_WM_.static_as_prefix());

#[export_root_module]
pub fn get_library() -> NeonLib_Ref {
    MODULE
}

#[sabi_extern_fn]
fn hash() -> RString {
    "".into()
}

#[sabi_extern_fn]
fn get_version() -> RString {
    "".into()
}

#[sabi_extern_fn]
fn init_config(params: &RStr) -> RResult<BoxedConfig<'static>, RString> {
    fn internal(params: &str) -> Result<Config, NeonError> {
        let api_config = serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        create_from_api_comnfig(&api_config)
    }
    internal(params.as_str())
        .map(DynTrait::from_value)
        .map_err(neon_error_to_rstring)
        .into()
}

#[sabi_extern_fn]
fn init_context(config: &BoxedConfig, params: &RStr) -> RResult<BoxedContext<'static>, RString> {
    fn internal(config: &Config, params: &str) -> Result<Context, NeonError> {
        let slot = serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let (rpc_client, blocking_rpc_client) = build_rpc_client(config, slot)?;
        let signer = NeonSigner::new(config)?;
        Ok(neon_lib::context::create(
            rpc_client,
            signer,
            blocking_rpc_client,
        ))
    }
    internal(config.downcast_as().unwrap(), params.as_str())
        .map(DynTrait::from_value)
        .map_err(neon_error_to_rstring)
        .into()
}

#[sabi_extern_fn]
fn init_hash_context<'a>(
    config: &'a BoxedConfig,
    params: &'a RStr,
) -> BorrowingFfiFuture<'a, RResult<BoxedContext<'static>, RString>> {
    async fn internal(config: &Config, params: &str) -> Result<Context, NeonError> {
        let slot = serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let (rpc_client, blocking_rpc_client) = build_hash_rpc_client(config, slot).await?;
        let signer = NeonSigner::new(config)?;
        Ok(neon_lib::context::create(
            rpc_client,
            signer,
            blocking_rpc_client,
        ))
    }
    async move {
        internal(config.downcast_as::<Config>().unwrap(), params.as_str())
            .await
            .map(DynTrait::from_value)
            .map_err(neon_error_to_rstring)
            .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn invoke<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    method: &'a RStr,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async move {
        dispatch(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            method.as_str(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(neon_error_to_rstring)
        .into()
    }
    .into_ffi()
}

async fn dispatch(
    config: &Config,
    context: &Context,
    method: &str,
    params: &str,
) -> Result<String, NeonError> {
    match method {
        "cancel_trx" => {
            let storage_account =
                serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result = cancel_trx::execute(config, context, &storage_account).await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "collect_treasury" => {
            let result = collect_treasury::execute(config, context).await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "create_ether_account" => {
            let ether_address =
                serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result = create_ether_account::execute(config, context, &ether_address).await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "deposit" => {
            let (amount, ether_address) =
                serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result = deposit::execute(config, context, amount, &ether_address).await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "emulate" => {
            let (tx_params, token, chain, steps, accounts, solana_accounts): (
                _,
                _,
                _,
                _,
                Vec<_>,
                Vec<_>,
            ) = serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result = emulate::execute(
                config,
                context,
                tx_params,
                token,
                chain,
                steps,
                &accounts,
                &solana_accounts,
            )
            .await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "get_ether_account_data" => {
            let ether_address =
                serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result = get_ether_account_data::execute(config, context, &ether_address).await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "get_neon_elf" => {
            let program_location: Option<String> =
                serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result =
                get_neon_elf::execute(config, context, program_location.as_deref()).await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "get_storage_at" => {
            let (ether_address, index) =
                serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result = get_storage_at::execute(config, context, ether_address, &index).await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        "init_environment" => {
            let (send_trx, force, keys_dir, file): (_, _, Option<String>, Option<String>) =
                serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
            let result = init_environment::execute(
                config,
                context,
                send_trx,
                force,
                keys_dir.as_deref(),
                file.as_deref(),
            )
            .await?;
            Ok(serde_json::to_string(&result).unwrap())
        }
        _ => Err(params_to_neon_error(method)),
    }
}
