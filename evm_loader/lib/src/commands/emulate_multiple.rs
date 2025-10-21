use std::collections::HashMap;

use solana_sdk::pubkey::Pubkey;

use crate::{
    config::DbConfig,
    solana_simulator::instruction_error_to_string,
    tracing::tracers::TracerTypeEnum,
    types::{AccountInfoLevel, EmulateMultipleRequest, EmulateRequest, SerializedAccount},
    NeonResult,
};

use super::{emulate::EmulateResponse, get_config::BuildConfigSimulator};

/// Executes Solana simulation and checks for errors in instruction execution results
async fn simulate_preparatory_instructions(
    rpc: &impl BuildConfigSimulator,
    solana_tx: crate::types::SimulateSolanaRequest,
) -> NeonResult<crate::solana_simulator::SolanaSimulator> {
    let instructions = solana_tx.instructions.clone();

    let (result, simulator) = super::simulate_solana::execute(rpc, solana_tx).await?;

    let instructions = result
        .instructions
        .into_iter()
        .zip(instructions.into_iter());

    for (result, instruction) in instructions {
        let Some(error) = result.error else {
            continue; // Skip successful instructions
        };

        let error = instruction_error_to_string(instruction.program_id, error);
        let error = evm_loader::error::Error::ExternalCallFailed(instruction.program_id, error);
        return Err(error.into());
    }

    Ok(simulator)
}

pub async fn execute(
    rpc: &impl BuildConfigSimulator,
    db_config: Option<&DbConfig>,
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

    let simulator = simulate_preparatory_instructions(rpc, request.solana_tx).await?;
    for (key, account) in simulator.into_accounts() {
        overrides.insert(key, Some(account.into()));
    }

    for tx in request.tx {
        let single_emulate_request = EmulateRequest {
            tx,
            step_limit: request.step_limit,
            account_limit: request.account_limit,
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

        let accounts = response.accounts_data.take().unwrap_or_default();
        for (pubkey, account) in accounts {
            overrides.insert(pubkey, Some(account));
        }

        responses.push(response);
    }

    Ok(responses)
}
