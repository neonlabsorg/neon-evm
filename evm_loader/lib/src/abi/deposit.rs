use solana_client::nonblocking::rpc_client::RpcClient;

use super::params_to_neon_error;
use crate::commands::deposit::{self, DepositReturn};
use crate::{types::request_models::DepositRequest, NeonResult};
use crate::{Config, Context};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<DepositReturn> {
    let signer = context.signer()?;
    let rpc_client = context
        .rpc_client
        .as_any()
        .downcast_ref::<RpcClient>()
        .unwrap();

    let params: DepositRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    deposit::execute(
        rpc_client,
        config.evm_loader,
        signer.as_ref(),
        params.amount,
        &params.ether,
    )
    .await
}
