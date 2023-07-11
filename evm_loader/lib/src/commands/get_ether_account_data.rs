use evm_loader::{account::EthereumAccount, types::Address};
use serde::{Deserialize, Serialize};

use crate::{
    account_storage::{account_info, EmulatorAccountStorage},
    errors::NeonError,
    Config, Context, NeonResult,
};
use std::fmt;

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for GetEtherAccountDataReturn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "(solana_address: {:?}, address: {:?}, trx_count: {:?}, balance: {:?}, code_size: {:?}, ...)",
            self.solana_address, self.address, self.trx_count, self.balance, self.code_size)
    }
}

pub async fn execute(
    config: &Config,
    context: &Context,
    ether_address: &Address,
) -> NeonResult<GetEtherAccountDataReturn> {
    match EmulatorAccountStorage::get_account_from_solana(config, context, ether_address).await {
        (solana_address, Some(mut acc)) => {
            let acc_info = account_info(&solana_address, &mut acc);
            let account_data =
                EthereumAccount::from_account(&config.evm_loader, &acc_info).unwrap();
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
