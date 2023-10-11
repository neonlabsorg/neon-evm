use super::params_to_neon_error;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::get_neon_elf::{self, GetNeonElfReturn};
use crate::rpc::Rpc;
use crate::Config;
use crate::{types::GetNeonElfRequest, NeonResult};

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    config: &Config,
    params: &str,
) -> NeonResult<GetNeonElfReturn> {
    let params: GetNeonElfRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_neon_elf::execute(config, rpc, params.program_location.as_deref()).await
}
