use super::params_to_neon_error;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::get_transaction_tree::{self, GetTreeResponse};
use crate::config::APIOptions;
use crate::rpc::Rpc;
use crate::{types::GetTransactionTreeRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &APIOptions,
    params: &str,
) -> NeonResult<GetTreeResponse> {
    let params: GetTransactionTreeRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_transaction_tree::execute(rpc, &config.evm_loader, params.origin, params.nonce).await
}
