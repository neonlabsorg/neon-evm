use evm_loader::{
    account::{legacy::LegacyEtherData, ContractAccount},
    types::Address,
};
use serde::{Deserialize, Serialize};
use solana_sdk::{account::Account, pubkey::Pubkey};

use crate::{account_storage::account_info, rpc::Rpc, NeonResult};

use serde_with::{hex::Hex, serde_as, DisplayFromStr};

use super::get_config::BuildConfigSimulator;

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct GetContractResponse {
    #[serde_as(as = "DisplayFromStr")]
    pub solana_address: Pubkey,
    pub chain_id: Option<u64>,
    #[serde_as(as = "Hex")]
    pub code: Vec<u8>,
}

impl GetContractResponse {
    #[must_use]
    pub const fn empty(solana_address: Pubkey) -> Self {
        Self {
            solana_address,
            chain_id: None,
            code: vec![],
        }
    }
}

fn read_legacy_account(
    program_id: &Pubkey,
    legacy_chain_id: u64,
    solana_address: Pubkey,
    mut account: Account,
) -> GetContractResponse {
    let account_info = account_info(&solana_address, &mut account);
    let Ok(contract) = LegacyEtherData::from_account(program_id, &account_info) else {
        return GetContractResponse::empty(solana_address);
    };

    let chain_id = Some(legacy_chain_id);
    let code = contract.read_code(&account_info);

    GetContractResponse {
        solana_address,
        chain_id,
        code,
    }
}

fn read_account(
    program_id: &Pubkey,
    legacy_chain_id: u64,
    solana_address: Pubkey,
    account: Option<Account>,
) -> GetContractResponse {
    let Some(mut account) = account else {
        return GetContractResponse::empty(solana_address);
    };

    let account_info = account_info(&solana_address, &mut account);
    let Ok(contract) = ContractAccount::from_account(program_id, account_info) else {
        return read_legacy_account(program_id, legacy_chain_id, solana_address, account);
    };

    let chain_id = Some(contract.chain_id());
    let code = contract.code().to_vec();

    GetContractResponse {
        solana_address,
        chain_id,
        code,
    }
}

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    program_id: &Pubkey,
    accounts: &[Address],
) -> NeonResult<Vec<GetContractResponse>> {
    let legacy_chain_id = super::get_config::read_legacy_chain_id(rpc, *program_id).await?;

    let pubkeys: Vec<_> = accounts
        .iter()
        .map(|a| a.find_solana_address(program_id).0)
        .collect();

    let accounts = rpc.get_multiple_accounts(&pubkeys).await?;

    let mut result = Vec::with_capacity(accounts.len());
    for (key, account) in pubkeys.into_iter().zip(accounts) {
        let response = read_account(program_id, legacy_chain_id, key, account);
        result.push(response);
    }

    Ok(result)
}
