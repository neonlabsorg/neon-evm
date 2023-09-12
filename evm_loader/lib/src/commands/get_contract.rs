use evm_loader::{account_storage::AccountStorage, types::Address};
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;

use crate::{account_storage::EmulatorAccountStorage, rpc::Rpc, NeonResult};

use serde_with::{hex::Hex, serde_as, DisplayFromStr};

#[serde_as]
#[derive(Debug, Serialize)]
pub struct GetContractResponse {
    #[serde_as(as = "DisplayFromStr")]
    pub solana_address: Pubkey,
    pub chain_id: Option<u64>,
    #[serde_as(as = "Hex")]
    pub code: Vec<u8>,
}

pub async fn execute(
    rpc_client: &dyn Rpc,
    program_id: &Pubkey,
    accounts: &[Address],
) -> NeonResult<Vec<GetContractResponse>> {
    let solana_accounts: Vec<_> = accounts
        .iter()
        .map(|a| a.find_solana_address(program_id).0)
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
    for (address, pubkey) in accounts.iter().zip(solana_accounts) {
        result.push(GetContractResponse {
            solana_address: pubkey,
            chain_id: backend.contract_chain_id(*address).await.ok(),
            code: backend.code(*address).await.to_vec(),
        });
    }

    Ok(result)
}
