use std::collections::HashSet;

use bincode::Options;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use solana_program_runtime::compute_budget::ComputeBudget;
use solana_runtime::runtime_config::RuntimeConfig;
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};

use crate::{
    rpc::Rpc, solana_simulator::SolanaSimulator, types::SimulateSolanaRequest, NeonResult,
};

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct SimulateSolanaTransactionResult {
    pub error: Option<solana_sdk::transaction::TransactionError>,
    pub logs: Vec<String>,
    pub executed_units: u64,
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct SimulateSolanaResponse {
    transactions: Vec<SimulateSolanaTransactionResult>,
}

pub async fn execute(
    rpc: &impl Rpc,
    request: SimulateSolanaRequest,
) -> NeonResult<SimulateSolanaResponse> {
    let mut transactions: Vec<VersionedTransaction> = vec![];
    for data in request.transactions {
        let tx = bincode::options()
            .with_fixint_encoding()
            .allow_trailing_bytes()
            .deserialize(&data)?;

        transactions.push(tx);
    }

    let mut accounts: HashSet<Pubkey> = HashSet::<Pubkey>::new();
    for tx in &transactions {
        let keys = tx.message.static_account_keys();
        accounts.extend(keys);
    }

    let config = RuntimeConfig {
        compute_budget: request.compute_units.map(ComputeBudget::new),
        log_messages_bytes_limit: Some(100 * 1024),
        transaction_account_lock_limit: request.account_limit,
    };
    let mut simulator = SolanaSimulator::new_with_config(rpc, config).await?;

    let accounts: Vec<Pubkey> = accounts.into_iter().collect();
    simulator.sync_accounts(rpc, &accounts).await?;

    simulator.replace_blockhash(&request.blockhash.into());

    let results = simulator
        .process_multiple_transactions(transactions)?
        .into_iter()
        .map(|r| SimulateSolanaTransactionResult {
            error: r.status.err(),
            logs: r.log_messages.unwrap_or_default(),
            executed_units: r.executed_units,
        })
        .collect();

    Ok(SimulateSolanaResponse {
        transactions: results,
    })
}
