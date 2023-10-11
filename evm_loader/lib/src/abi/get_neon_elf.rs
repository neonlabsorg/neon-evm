use super::params_to_neon_error;
use crate::commands::get_neon_elf::{self, GetNeonElfReturn};
use crate::{types::request_models::GetNeonElfRequest, NeonResult};
use crate::{Config, Context};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<GetNeonElfReturn> {
    let params: GetNeonElfRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    get_neon_elf::execute(config, context, params.program_location.as_deref()).await
}
