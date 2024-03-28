use super::params_to_neon_error;
use crate::commands::simulate_solana::{self, SimulateSolanaResponse};
use crate::config::APIOptions;
use crate::rpc::Rpc;
use crate::types::SimulateSolanaRequest;
use crate::NeonResult;

pub async fn execute(
    rpc: &impl Rpc,
    _config: &APIOptions,
    params: &str,
) -> NeonResult<SimulateSolanaResponse> {
    let params: SimulateSolanaRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    simulate_solana::execute(rpc, params).await
}
