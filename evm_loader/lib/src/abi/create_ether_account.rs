use solana_client::nonblocking::rpc_client::RpcClient;

use super::params_to_neon_error;
use crate::commands::create_ether_account::CreateEtherAccountReturn;
use crate::{
    commands::create_ether_account::{self},
    context::Context,
    types::request_models::CreateEtherAccountRequest,
    Config, NeonResult,
};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<CreateEtherAccountReturn> {
    let signer = context.signer()?;
    let rpc_client = context
        .rpc_client
        .as_any()
        .downcast_ref::<RpcClient>()
        .unwrap();

    let params: CreateEtherAccountRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    create_ether_account::execute(
        rpc_client,
        config.evm_loader,
        signer.as_ref(),
        &params.ether,
    )
    .await
}
