use super::params_to_neon_error;
use crate::commands::init_environment::{self, InitEnvironmentReturn};
use crate::{types::request_models::InitEnvironmentRequest, NeonResult};
use crate::{Config, Context};

pub async fn execute(
    context: &Context<'_>,
    config: &Config,
    params: &str,
) -> NeonResult<InitEnvironmentReturn> {
    let params: InitEnvironmentRequest =
        serde_json::from_str(params).map_err(|_| params_to_neon_error(params))?;

    init_environment::execute(
        config,
        context,
        params.send_trx,
        params.force,
        params.keys_dir.as_deref(),
        params.file.as_deref(),
    )
    .await
}
