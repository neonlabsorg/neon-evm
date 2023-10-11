use crate::{
    commands::collect_treasury::{self, CollectTreasuryReturn},
    rpc::CloneRpcClient,
    Config, NeonResult,
};
use solana_sdk::signer::Signer;

pub async fn execute(
    rpc: &CloneRpcClient,
    signer: &dyn Signer,
    config: &Config,
    _params: &str,
) -> NeonResult<CollectTreasuryReturn> {
    collect_treasury::execute(config, rpc, signer).await
}
