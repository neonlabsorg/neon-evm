use ethnum::U256;
use evm_loader::account::BalanceAccount;
use evm_loader::platform::Platform;
use evm_loader::types::Address;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

use crate::rpc::Rpc;
use crate::{emulator_platform::EmulatorPlatform, types::BalanceAddress, NeonResult};

use serde_with::{serde_as, DisplayFromStr};

use super::get_config::BuildConfigSimulator;

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum BalanceStatus {
    Ok,
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
    pub user_pubkey: Option<Pubkey>,
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
            user_pubkey: None,
        }
    }

    #[must_use]
    pub fn new(program_id: &Pubkey, account: &BalanceAccount) -> Self {
        let address = account.address();
        let (contract_solana_address, _) = address.find_solana_address(program_id);

        Self {
            solana_address: account.pubkey(),
            contract_solana_address,
            trx_count: account.nonce(),
            balance: account.balance(),
            status: BalanceStatus::Ok,
            user_pubkey: account.solana_address(),
        }
    }
}

pub async fn execute(
    rpc: &impl Rpc,
    program_id: &Pubkey,
    address: &[BalanceAddress],
) -> NeonResult<Vec<GetBalanceResponse>> {
    let pubkeys: Vec<_> = address.iter().map(|a| a.find_pubkey(program_id)).collect();
    let platform = EmulatorPlatform::new(rpc, *program_id, &[], &pubkeys).await?;

    let mut result = Vec::with_capacity(address.len());
    for a in address {
        let balance = platform.get_balance(a.address, a.chain_id).await?;
        let response = balance.map_or_else(
            || GetBalanceResponse::empty(program_id, a),
            |balance| GetBalanceResponse::new(program_id, &balance),
        );

        result.push(response);
    }

    Ok(result)
}

pub async fn execute_with_pubkey(
    rpc: &impl BuildConfigSimulator,
    program_id: &Pubkey,
    pubkeys: &[Pubkey],
) -> NeonResult<Vec<GetBalanceResponse>> {
    let chain_id = super::get_config::read_sol_chain_id(rpc, *program_id).await?;

    let addresses = pubkeys
        .iter()
        .map(Address::from_solana_address)
        .map(|address| BalanceAddress { address, chain_id })
        .collect::<Vec<_>>();

    execute(rpc, program_id, &addresses).await
}
