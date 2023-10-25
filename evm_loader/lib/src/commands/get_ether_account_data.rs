use ethnum::U256;
use evm_loader::{account::EthereumAccount, types::Address};
use serde::{Deserialize, Serialize};
use solana_sdk::{account::Account, pubkey::Pubkey};
use std::fmt::{Display, Formatter};

use crate::{
    account_storage::{account_info, EmulatorAccountStorage},
    errors::NeonError,
    rpc::Rpc,
    NeonResult,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetEtherAccountDataReturn {
    pub solana_address: String,
    pub address: Address,
    pub bump_seed: u8,
    pub trx_count: u64,
    pub rw_blocked: bool,
    pub balance: String,
    pub generation: u32,
    pub code_size: u32,
    pub code: String,
}

impl GetEtherAccountDataReturn {
    pub fn new(
        program_id: &Pubkey,
        address: Address,
        pubkey: Pubkey,
        account: Option<Account>,
    ) -> Self {
        let Some(mut account) = account else {
            return Self::empty(address, pubkey);
        };

        let info = account_info(&pubkey, &mut account);
        let Ok(account_data) = EthereumAccount::from_account(program_id, &info) else {
            return Self::empty(address, pubkey);
        };

        let contract_code = account_data
            .contract_data()
            .map_or_else(Vec::new, |c| c.code().to_vec());

        GetEtherAccountDataReturn {
            solana_address: pubkey.to_string(),
            address,
            bump_seed: account_data.bump_seed,
            trx_count: account_data.trx_count,
            rw_blocked: account_data.rw_blocked,
            balance: account_data.balance.to_string(),
            generation: account_data.generation,
            code_size: account_data.code_size,
            code: hex::encode(contract_code),
        }
    }

    pub fn empty(address: Address, pubkey: Pubkey) -> Self {
        Self {
            solana_address: pubkey.to_string(),
            address,
            bump_seed: 0,
            trx_count: 0,
            rw_blocked: false,
            balance: U256::ZERO.to_string(),
            generation: 0,
            code_size: 0,
            code: String::new(),
        }
    }
}

impl Display for GetEtherAccountDataReturn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ address: {}, solana_address: {}, trx_count: {}, balance: {}, generation: {}, code_size: {} }}",
            self.address,
            self.solana_address,
            self.trx_count,
            self.balance,
            self.generation,
            self.code_size,
        )
    }
}

pub async fn execute(
    rpc_client: &dyn Rpc,
    evm_loader: &Pubkey,
    ether_address: &Address,
) -> NeonResult<GetEtherAccountDataReturn> {
    match EmulatorAccountStorage::get_account_from_solana(rpc_client, evm_loader, ether_address)
        .await
    {
        (solana_address, Some(mut acc)) => {
            let acc_info = account_info(&solana_address, &mut acc);
            let account_data = EthereumAccount::from_account(evm_loader, &acc_info).unwrap();
            let contract_code = account_data
                .contract_data()
                .map_or_else(Vec::new, |c| c.code().to_vec());

            Ok(GetEtherAccountDataReturn {
                solana_address: solana_address.to_string(),
                address: account_data.address,
                bump_seed: account_data.bump_seed,
                trx_count: account_data.trx_count,
                rw_blocked: account_data.rw_blocked,
                balance: account_data.balance.to_string(),
                generation: account_data.generation,
                code_size: account_data.code_size,
                code: hex::encode(contract_code),
            })
        }
        (solana_address, None) => Err(NeonError::AccountNotFound(solana_address)),
    }
}

pub async fn execute_batch(
    rpc_client: &dyn Rpc,
    evm_loader: &Pubkey,
    addresses: &[Address],
) -> NeonResult<Vec<GetEtherAccountDataReturn>> {
    let pubkeys: Vec<Pubkey> = addresses
        .iter()
        .map(|a| a.find_solana_address(evm_loader).0)
        .collect();

    let accounts = rpc_client.get_multiple_accounts(&pubkeys).await?;

    Ok(addresses
        .iter()
        .zip(pubkeys)
        .zip(accounts)
        .map(|((address, pubkey), account)| {
            GetEtherAccountDataReturn::new(evm_loader, *address, pubkey, account)
        })
        .collect())
}
