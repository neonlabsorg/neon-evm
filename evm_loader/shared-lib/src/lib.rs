use abi_stable::{
    export_root_module,
    prefix_type::WithMetadata,
    sabi_extern_fn,
    std_types::{RResult, RStr, RString},
    DynTrait,
};
use async_ffi::{BorrowingFfiFuture, FutureExt};
use neon_interface::{
    types::{BoxedConfig, BoxedContext, BoxedNeonError, RNeonResult},
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
    Config, Context, NeonError, NeonResult,
};

const _MODULE_WM_: &WithMetadata<NeonLib> = &WithMetadata::new(NeonLib {
    api_version: 1,
    hash,
    init_config,
    init_context,
    init_hash_context,
    cancel_trx,
    collect_treasury,
    create_ether_account,
    deposit,
    emulate,
    get_ether_account_data,
    get_neon_elf,
    get_storage_at,
    init_environment,
});

fn params_to_neon_error(params: &str) -> NeonError {
    NeonError::EnvironmentError(
        neon_lib::commands::init_environment::EnvironmentError::InvalidProgramParameter(
            params.into(),
        ),
    )
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
fn init_config(params: &RStr) -> RResult<BoxedConfig<'static>, BoxedNeonError<'static>> {
    fn internal(params: &str) -> Result<Config, NeonError> {
        let api_config = serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        create_from_api_comnfig(&api_config)
    }
    internal(params.as_str())
        .map(DynTrait::from_value)
        .map_err(DynTrait::from_value)
        .into()
}

#[sabi_extern_fn]
fn init_context(
    config: &BoxedConfig,
    params: &RStr,
) -> RResult<BoxedContext<'static>, BoxedNeonError<'static>> {
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
        .map_err(DynTrait::from_value)
        .into()
}

#[sabi_extern_fn]
fn init_hash_context<'a>(
    config: &'a BoxedConfig,
    params: &'a RStr,
) -> BorrowingFfiFuture<'a, RResult<BoxedContext<'static>, BoxedNeonError<'static>>> {
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
            .map_err(DynTrait::from_value)
            .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn cancel_trx<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
        let storage_account =
            serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let result = cancel_trx::execute(config, context, &storage_account).await?;
        Ok(serde_json::to_string(&result).unwrap())
    }
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn collect_treasury<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    _params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context) -> NeonResult<String> {
        let result = collect_treasury::execute(config, context).await?;
        Ok(serde_json::to_string(&result).unwrap())
    }
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn create_ether_account<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
        let ether_address =
            serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let result = create_ether_account::execute(config, context, &ether_address).await?;
        Ok(serde_json::to_string(&result).unwrap())
    }
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn deposit<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
        let (amount, ether_address) =
            serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let result = deposit::execute(config, context, amount, &ether_address).await?;
        Ok(serde_json::to_string(&result).unwrap())
    }
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn emulate<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
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
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn get_ether_account_data<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
        let ether_address =
            serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let result = get_ether_account_data::execute(config, context, &ether_address).await?;
        Ok(serde_json::to_string(&result).unwrap())
    }
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn get_neon_elf<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
        let program_location: Option<String> =
            serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let result = get_neon_elf::execute(config, context, program_location.as_deref()).await?;
        Ok(serde_json::to_string(&result).unwrap())
    }
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn get_storage_at<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
        let (ether_address, index) =
            serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;
        let result = get_storage_at::execute(config, context, ether_address, &index).await?;
        Ok(serde_json::to_string(&result).unwrap())
    }
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}

#[sabi_extern_fn]
fn init_environment<'a>(
    config: &'a BoxedConfig,
    context: &'a BoxedContext,
    params: &'a RStr,
) -> RNeonResult<'a> {
    async fn internal(config: &Config, context: &Context, params: &str) -> NeonResult<String> {
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
    async move {
        internal(
            config.downcast_as().unwrap(),
            context.downcast_as().unwrap(),
            params.as_str(),
        )
        .await
        .map(RString::from)
        .map_err(DynTrait::from_value)
        .into()
    }
    .into_ffi()
}
