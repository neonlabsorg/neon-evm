use ethnum::U256;
use evm_loader::account::BalanceAccount;
use serde::Serialize;
use solana_sdk::{account::Account, pubkey::Pubkey};

use crate::{account_storage::account_info, rpc::Rpc, types::BalanceAddress, NeonResult};

use serde_with::{serde_as, DisplayFromStr};

#[serde_as]
#[derive(Debug, Serialize)]
pub struct GetBalanceResponse {
    #[serde_as(as = "DisplayFromStr")]
    pub solana_address: Pubkey,
    pub trx_count: u64,
    pub balance: U256,
}

impl GetBalanceResponse {
    pub fn empty(solana_address: Pubkey) -> Self {
        Self {
            solana_address,
            trx_count: 0,
            balance: U256::ZERO,
        }
    }
}

fn read_account(
    program_id: &Pubkey,
    solana_address: Pubkey,
    account: Option<Account>,
) -> NeonResult<GetBalanceResponse> {
    let Some(mut account) = account else {
        return Ok(GetBalanceResponse::empty(solana_address));
    };

    let account_info = account_info(&solana_address, &mut account);
    let balance_account = BalanceAccount::from_account(program_id, account_info, None)?;

    Ok(GetBalanceResponse {
        solana_address,
        trx_count: balance_account.nonce(),
        balance: balance_account.balance(),
    })
}

pub async fn execute(
    rpc_client: &dyn Rpc,
    program_id: &Pubkey,
    accounts: &[BalanceAddress],
) -> NeonResult<Vec<GetBalanceResponse>> {
    let pubkeys: Vec<_> = accounts.iter().map(|a| a.find_pubkey(program_id)).collect();
    let accounts = rpc_client.get_multiple_accounts(&pubkeys).await?;

    let mut result = Vec::with_capacity(accounts.len());
    for (key, account) in pubkeys.into_iter().zip(accounts) {
        let response = read_account(program_id, key, account)?;
        result.push(response);
    }

    Ok(result)
}
