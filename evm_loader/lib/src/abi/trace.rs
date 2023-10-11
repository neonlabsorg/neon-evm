use serde_json::Value;

use super::params_to_neon_error;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::trace::{self};
use crate::rpc::Rpc;
use crate::Config;
use crate::{types::EmulateApiRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &Config,
    params: &str,
) -> NeonResult<Value> {
    let params: EmulateApiRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    trace::trace_transaction(rpc, config.evm_loader, params.body).await
}
