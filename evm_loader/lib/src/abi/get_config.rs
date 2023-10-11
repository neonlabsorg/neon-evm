use crate::commands::get_config::{self, BuildConfigSimulator, GetConfigResponse};
use crate::rpc::Rpc;
use crate::Config;
use crate::NeonResult;

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &Config,
    _params: &str,
) -> NeonResult<GetConfigResponse> {
    get_config::execute(rpc, config.evm_loader).await
}
