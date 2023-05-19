use serde_json::json;
use tide::{Request, Result};

use crate::{
    api_server::state::State,
    commands::trace::trace_block,
    context,
    types::{request_models::TraceNextBlockRequestModel, IndexerDb},
};

use super::{parse_emulation_params, process_result};

#[allow(clippy::unused_async)]
pub async fn trace_next_block(mut req: Request<State>) -> Result<serde_json::Value> {
    let trace_next_block_request: TraceNextBlockRequestModel =
        req.body_json().await.map_err(|e| {
            tide::Error::from_str(
                400,
                format!(
                    "Error on parsing transaction parameters request: {:?}",
                    e.to_string()
                ),
            )
        })?;

    let state = req.state();

    let signer = context::build_singer(&state.config).map_err(|e| {
        tide::Error::from_str(
            400,
            format!("Error on creating singer: {:?}", e.to_string()),
        )
    })?;

    let rpc_client = context::build_rpc_client(&state.config, Some(trace_next_block_request.slot))
        .map_err(|e| {
            tide::Error::from_str(
                400,
                format!("Error on creating rpc client: {:?}", e.to_string()),
            )
        })?;

    let context = context::create(rpc_client, signer);

    let (token, chain, steps, accounts, solana_accounts) = parse_emulation_params(
        &state.config,
        &context,
        &trace_next_block_request.emulation_params,
    );

    let indexer_db = IndexerDb::new(
        state
            .config
            .db_config
            .as_ref()
            .expect("db-config is required"),
    );
    let transactions = indexer_db.get_block_transactions(trace_next_block_request.slot + 1)?;

    process_result(
        &trace_block(
            context.rpc_client.as_ref(),
            state.config.evm_loader,
            transactions,
            token,
            chain,
            steps,
            state.config.commitment,
            &accounts,
            &solana_accounts,
            trace_next_block_request.trace_config.unwrap_or_default(),
        )
        .map(|result| json!(result)),
    )
}
