use evm_loader::{
    account::{legacy::LegacyEtherData, ContractAccount},
    types::Address,
};
use serde::Serialize;
use solana_sdk::{account::Account, pubkey::Pubkey};

use crate::{account_storage::account_info, rpc::Rpc, NeonResult};

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

impl GetContractResponse {
    pub fn empty(solana_address: Pubkey) -> Self {
        Self {
            solana_address,
            chain_id: None,
            code: vec![],
        }
    }
}

fn read_account(
    program_id: &Pubkey,
    solana_address: Pubkey,
    account: Option<Account>,
) -> NeonResult<GetContractResponse> {
    let Some(mut account) = account else {
        return Ok(GetContractResponse::empty(solana_address));
    };

    let account_info = account_info(&solana_address, &mut account);
    let (chain_id, code) =
        if let Ok(contract) = ContractAccount::from_account(program_id, account_info.clone()) {
            (Some(contract.chain_id()), contract.code().to_vec())
        } else if let Ok(contract) = LegacyEtherData::from_account(program_id, &account_info) {
            (None, contract.read_code(&account_info)?)
        } else {
            (None, vec![])
        };

    Ok(GetContractResponse {
        solana_address,
        chain_id,
        code,
    })
}

pub async fn execute(
    rpc_client: &dyn Rpc,
    program_id: &Pubkey,
    accounts: &[Address],
) -> NeonResult<Vec<GetContractResponse>> {
    let pubkeys: Vec<_> = accounts
        .iter()
        .map(|a| a.find_solana_address(program_id).0)
        .collect();

    let accounts = rpc_client.get_multiple_accounts(&pubkeys).await?;

    let mut result = Vec::with_capacity(accounts.len());
    for (key, account) in pubkeys.into_iter().zip(accounts) {
        let response = read_account(program_id, key, account)?;
        result.push(response);
    }

    Ok(result)
}
