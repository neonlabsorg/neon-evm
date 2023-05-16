use crate::{
    api_server::{request_models, state::State},
    context,
};
use evm_loader::types::Address;
use tide::{Request, Result};

use crate::commands::get_ether_account_data as GetEtherAccountDataCommand;
use request_models::GetEtherRequest;

use super::process_result;

#[allow(clippy::unused_async)]
pub async fn get_ether_account_data(req: Request<State>) -> Result<serde_json::Value> {
    let state = req.state();
    let get_ether: GetEtherRequest = req.query().unwrap_or_default();
    let address = Address::from_hex(get_ether.ether.unwrap_or_default().as_str())
        .map_err(|_| tide::Error::from_str(400, "address is incorrect"))?;

    let signer = context::build_singer(&state.config).map_err(|e| {
        tide::Error::from_str(
            400,
            format!("Error on creating singer: {:?}", e.to_string()),
        )
    })?;

    let rpc_client = context::build_rpc_client(&state.config, get_ether.slot).map_err(|e| {
        tide::Error::from_str(
            400,
            format!("Error on creating rpc client: {:?}", e.to_string()),
        )
    })?;

    let context = context::create(rpc_client, signer);

    process_result(&GetEtherAccountDataCommand::execute(
        &state.config,
        &context,
        &address,
    ))
}
