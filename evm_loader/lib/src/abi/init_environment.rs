use super::params_to_neon_error;
use crate::commands::init_environment::{self, InitEnvironmentReturn};
use crate::rpc::CloneRpcClient;
use crate::Config;
use crate::{types::InitEnvironmentRequest, NeonResult};
use solana_sdk::signer::Signer;

pub async fn execute(
    rpc: &CloneRpcClient,
    signer: &dyn Signer,
    config: &Config,
    params: &str,
) -> NeonResult<InitEnvironmentReturn> {
    let params: InitEnvironmentRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    init_environment::execute(
        config,
        rpc,
        signer,
        params.send_trx,
        params.force,
        params.keys_dir.as_deref(),
        params.file.as_deref(),
    )
    .await
}
