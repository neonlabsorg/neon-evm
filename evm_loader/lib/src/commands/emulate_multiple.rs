use std::collections::HashMap;

use solana_sdk::pubkey::Pubkey;

use crate::{
    config::DbConfig,
    tracing::tracers::TracerTypeEnum,
    types::{AccountInfoLevel, EmulateMultipleRequest, EmulateRequest, SerializedAccount},
    NeonResult,
};

use super::{emulate::EmulateResponse, get_config::BuildConfigSimulator};

pub async fn execute(
    rpc: &impl BuildConfigSimulator,
    db_config: &Option<DbConfig>,
    program_id: &Pubkey,
    request: EmulateMultipleRequest,
) -> NeonResult<Vec<EmulateResponse>> {
    let mut responses = vec![];

    let accounts = rpc
        .get_multiple_accounts(&request.accounts)
        .await?
        .into_iter()
        .map(|a| a.map(SerializedAccount::from));

    let mut overrides: HashMap<_, _> = request.accounts.into_iter().zip(accounts).collect();

    let (_, simulator) = super::simulate_solana::execute(rpc, request.solana_tx).await?;
    for (key, account) in simulator.into_accounts() {
        overrides.insert(key, Some(account.into()));
    }

    for tx in request.tx {
        let single_emulate_request = EmulateRequest {
            tx,
            step_limit: request.step_limit,
            chains: Option::clone(&request.chains),
            trace_config: None,
            accounts: vec![],
            solana_overrides: Some(overrides.clone()),
            provide_account_info: Some(AccountInfoLevel::All),
            execution_map: None,
        };

        let (mut response, _) = super::emulate::execute(
            rpc,
            db_config,
            program_id,
            single_emulate_request,
            None::<TracerTypeEnum>,
        )
        .await?;

        let accounts = response.accounts_data.take().unwrap();
        for account in accounts {
            overrides.insert(account.pubkey, Some(account.into()));
        }

        responses.push(response);
    }

    Ok(responses)
}
