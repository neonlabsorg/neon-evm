use serde_json::json;
use tide::{Request, Result};

use crate::{
    api_server::state::State,
    commands::emulate as EmulateCommand,
    context,
    types::{request_models::EmulateRequestModel, trace::TraceCallConfig},
};

use super::{parse_emulation_params, parse_tx, process_result};

#[allow(clippy::unused_async)]
pub async fn emulate(mut req: Request<State>) -> Result<serde_json::Value> {
    let emulate_request: EmulateRequestModel = req.body_json().await.map_err(|e| {
        tide::Error::from_str(
            400,
            format!(
                "Error on parsing transaction parameters request: {:?}",
                e.to_string()
            ),
        )
    })?;

    let state = req.state();

    let tx = parse_tx(&emulate_request.tx_params);

    let signer = context::build_singer(&state.config).map_err(|e| {
        tide::Error::from_str(
            400,
            format!("Error on creating singer: {:?}", e.to_string()),
        )
    })?;

    let rpc_client =
        context::build_rpc_client(&state.config, emulate_request.slot).map_err(|e| {
            tide::Error::from_str(
                400,
                format!("Error on creating rpc client: {:?}", e.to_string()),
            )
        })?;

    let context = context::create(rpc_client, signer);

    let (token, chain, steps, accounts, solana_accounts) =
        parse_emulation_params(&state.config, &context, &emulate_request.emulation_params);

    process_result(
        &EmulateCommand::execute(
            context.rpc_client.as_ref(),
            state.config.evm_loader,
            tx,
            token,
            chain,
            steps,
            state.config.commitment,
            &accounts,
            &solana_accounts,
            TraceCallConfig::default(),
        )
        .map(|result| json!(result)),
    )
}
