use crate::commands::get_config::{self, BuildConfigSimulator, GetConfigResponse};
use crate::config::APIOptions;
use crate::rpc::Rpc;
use crate::NeonResult;

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &APIOptions,
    _params: &str,
) -> NeonResult<GetConfigResponse> {
    get_config::execute(rpc, config.evm_loader).await
}
