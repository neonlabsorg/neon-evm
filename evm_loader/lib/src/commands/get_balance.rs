use ethnum::U256;
use evm_loader::account_storage::AccountStorage;
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;

use crate::{account_storage::EmulatorAccountStorage, rpc::Rpc, types::BalanceAddress, NeonResult};

use serde_with::{serde_as, DisplayFromStr};

#[serde_as]
#[derive(Debug, Serialize)]
pub struct GetBalanceResponse {
    #[serde_as(as = "DisplayFromStr")]
    pub solana_address: Pubkey,
    pub trx_count: u64,
    pub balance: U256,
}

pub async fn execute(
    rpc_client: &dyn Rpc,
    program_id: &Pubkey,
    accounts: &[BalanceAddress],
) -> NeonResult<Vec<GetBalanceResponse>> {
    let solana_accounts: Vec<_> = accounts
        .iter()
        .map(|a| a.solana_address(program_id))
        .collect();

    let backend = EmulatorAccountStorage::with_accounts(
        rpc_client,
        *program_id,
        &solana_accounts,
        None,
        None,
    )
    .await?;

    let mut result = Vec::new();
    for (a, pubkey) in accounts.iter().zip(solana_accounts) {
        result.push(GetBalanceResponse {
            solana_address: pubkey,
            trx_count: backend.nonce(a.address, a.chain_id).await,
            balance: backend.balance(a.address, a.chain_id).await,
        });
    }

    Ok(result)
}
