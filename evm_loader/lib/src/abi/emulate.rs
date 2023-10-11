use super::params_to_neon_error;
use crate::commands::emulate::{self, EmulateResponse};
use crate::commands::get_config::BuildConfigSimulator;
use crate::config::APIOptions;
use crate::rpc::Rpc;
use crate::tracing::tracers::TracerTypeEnum;
use crate::{types::EmulateApiRequest, NeonResult};
use serde_json::Value;

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &APIOptions,
    params: &str,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let params: EmulateApiRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    emulate::execute(rpc, config.evm_loader, params.body, None::<TracerTypeEnum>).await
}
