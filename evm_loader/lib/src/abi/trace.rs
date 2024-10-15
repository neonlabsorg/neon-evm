use serde_json::Value;

use super::params_to_neon_error;
use crate::commands::emulate::EmulateResponse;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::trace::{self};
use crate::config::APIOptions;
use crate::rpc::Rpc;
use crate::{types::EmulateApiRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &APIOptions,
    params: &str,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let params: EmulateApiRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    trace::trace_transaction(rpc, &config.db_config, &config.evm_loader, params.body).await
}
