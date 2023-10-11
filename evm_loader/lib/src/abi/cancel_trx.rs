use super::params_to_neon_error;
use crate::commands::cancel_trx::{self, CancelTrxReturn};
use crate::Config;
use crate::{types::CancelTrxRequest, NeonResult};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signer::Signer;

pub async fn execute(
    rpc: &RpcClient,
    signer: &dyn Signer,
    config: &Config,
    params: &str,
) -> NeonResult<CancelTrxReturn> {
    let params: CancelTrxRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    cancel_trx::execute(rpc, signer, config.evm_loader, &params.storage_account).await
}
