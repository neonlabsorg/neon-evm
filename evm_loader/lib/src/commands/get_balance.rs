#![allow(clippy::future_not_send)]

use ethnum::U256;
use evm_loader::account::legacy::LegacyEtherData;
use evm_loader::account::BalanceAccount;
use serde::{Deserialize, Serialize};
use solana_sdk::{account::Account, pubkey::Pubkey};

use crate::{account_storage::account_info, rpc::Rpc, types::BalanceAddress, NeonResult};

use serde_with::{serde_as, DisplayFromStr};

use super::get_config::BuildConfigSimulator;

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum BalanceStatus {
    Ok,
    Legacy,
    Empty,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct GetBalanceResponse {
    #[serde_as(as = "DisplayFromStr")]
    pub solana_address: Pubkey,
    #[serde_as(as = "DisplayFromStr")]
    pub contract_solana_address: Pubkey,
    pub trx_count: u64,
    pub balance: U256,
    pub status: BalanceStatus,
}

impl GetBalanceResponse {
    #[must_use]
    pub fn empty(program_id: &Pubkey, address: &BalanceAddress) -> Self {
        Self {
            solana_address: address.find_pubkey(program_id),
            contract_solana_address: address.find_contract_pubkey(program_id),
            trx_count: 0,
            balance: U256::ZERO,
            status: BalanceStatus::Empty,
        }
    }
}

fn read_account(
    program_id: &Pubkey,
    address: &BalanceAddress,
    mut account: Account,
) -> NeonResult<GetBalanceResponse> {
    let solana_address = address.find_pubkey(program_id);

    let account_info = account_info(&solana_address, &mut account);
    let balance_account = BalanceAccount::from_account(program_id, account_info)?;

    Ok(GetBalanceResponse {
        solana_address,
        contract_solana_address: address.find_contract_pubkey(program_id),
        trx_count: balance_account.nonce(),
        balance: balance_account.balance(),
        status: BalanceStatus::Ok,
    })
}

fn read_legacy_account(
    program_id: &Pubkey,
    address: &BalanceAddress,
    mut account: Account,
) -> NeonResult<GetBalanceResponse> {
    let solana_address = address.find_pubkey(program_id);
    let contract_solana_address = address.find_contract_pubkey(program_id);

    let account_info = account_info(&contract_solana_address, &mut account);
    let balance_account = LegacyEtherData::from_account(program_id, &account_info)?;

    Ok(GetBalanceResponse {
        solana_address,
        contract_solana_address,
        trx_count: balance_account.trx_count,
        balance: balance_account.balance,
        status: BalanceStatus::Legacy,
    })
}

pub async fn execute(
    rpc: &(impl Rpc + BuildConfigSimulator),
    program_id: &Pubkey,
    address: &[BalanceAddress],
) -> NeonResult<Vec<GetBalanceResponse>> {
    let legacy_chain_id = super::get_config::read_legacy_chain_id(rpc, *program_id).await?;

    let mut response: Vec<Option<GetBalanceResponse>> = vec![None; address.len()];
    let mut missing: Vec<BalanceAddress> = Vec::with_capacity(address.len());

    // Download accounts
    let pubkeys: Vec<_> = address.iter().map(|a| a.find_pubkey(program_id)).collect();
    let accounts = rpc.get_multiple_accounts(&pubkeys).await?;

    for (i, account) in accounts.into_iter().enumerate() {
        if let Some(account) = account {
            let balance = read_account(program_id, &address[i], account)?;
            response[i] = Some(balance);
        } else if address[i].chain_id == legacy_chain_id {
            missing.push(address[i]);
        } else {
            let balance = GetBalanceResponse::empty(program_id, &address[i]);
            response[i] = Some(balance);
        }
    }

    // Download missing accounts from legacy addresses
    let pubkeys: Vec<_> = missing
        .iter()
        .map(|a| a.find_contract_pubkey(program_id))
        .collect();
    let mut accounts = rpc.get_multiple_accounts(&pubkeys).await?;

    let mut j = 0_usize;
    for i in 0..response.len() {
        if response[i].is_some() {
            continue;
        }

        assert_eq!(address[i], missing[j]);

        let address = missing[j];
        let account = accounts[j].take();
        j += 1;

        let Some(account) = account else {
            continue;
        };
        let Ok(balance) = read_legacy_account(program_id, &address, account) else {
            continue;
        };
        response[i] = Some(balance);
    }

    // Treat still missing accounts as empty
    let mut result = Vec::with_capacity(response.len());
    for (i, balance) in response.into_iter().enumerate() {
        let balance = balance.unwrap_or_else(|| GetBalanceResponse::empty(program_id, &address[i]));
        result.push(balance);
    }

    Ok(result)
}
