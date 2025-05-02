use ethnum::U256;
use evm_loader::{
    account::{
        Holder, StateAccount, StateFinalizedAccount, TAG_HOLDER, TAG_SCHEDULED_STATE_CANCELLED,
        TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE, TAG_STATE_FINALIZED,
    },
    types::{Address, Transaction, TrxView},
};
use serde::{Deserialize, Serialize};
use solana_sdk::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};
use std::fmt::Display;

use crate::{account_storage::account_info, rpc::Rpc, types::TxParams, NeonResult};

use serde_with::{hex::Hex, serde_as, skip_serializing_none, DisplayFromStr};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum Status {
    #[default]
    Empty,
    Error(String),
    Holder,
    Active,
    Finalized,
    ScheduledFinalized,
    ScheduledCanceled,
}

#[serde_as]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AccountMeta {
    pub is_writable: bool,
    #[serde_as(as = "DisplayFromStr")]
    pub key: Pubkey,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GetHolderResponse {
    pub status: Status,
    pub len: Option<usize>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub owner: Option<Pubkey>,

    #[serde_as(as = "Option<Hex>")]
    pub tx: Option<[u8; 32]>,
    pub tx_data: Option<TxParams>,
    pub tx_type: Option<u8>,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
    pub chain_id: Option<u64>,
    pub origin: Option<Address>,
    pub tree_account: Option<Pubkey>,

    // (block_timestamp, block_number)
    pub block_params: Option<(U256, U256)>,

    #[serde_as(as = "Option<Vec<DisplayFromStr>>")]
    pub accounts: Option<Vec<Pubkey>>,

    pub steps_executed: u64,
}

impl GetHolderResponse {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            status: Status::Empty,
            ..Self::default()
        }
    }

    pub fn error<T: Display>(error: T) -> Self {
        Self {
            status: Status::Error(error.to_string()),
            ..Self::default()
        }
    }
}

pub fn read_holder(program_id: &Pubkey, info: &AccountInfo) -> NeonResult<GetHolderResponse> {
    let data_len = info.data_len();

    match evm_loader::account::tag(program_id, info)? {
        TAG_HOLDER => {
            let holder = Holder::from_account(program_id, info)?;

            Ok(GetHolderResponse {
                status: Status::Holder,
                len: Some(data_len),
                owner: Some(holder.owner()),
                tx: Some(holder.transaction_hash()),
                // Holder may not yet contain the transaction and empty rlp panics.
                // TODO: check the behavior.
                tx_type: Some(0),
                ..GetHolderResponse::default()
            })
        }
        TAG_STATE_FINALIZED => {
            let state = StateFinalizedAccount::from_account(program_id, info)?;

            Ok(GetHolderResponse {
                status: Status::Finalized,
                len: Some(data_len),
                owner: Some(state.owner()),
                tx: Some(state.trx_hash()),
                // transaction_type, max_fee_per_gas and max_priority_fee_per_gas are not needed
                // when transaction is already finalized.
                // Also, the data about transaction is already not in the holder anymore.
                // We explicitly set tx_type=0 to indicate that there shouldn't be new gas params.
                tx_type: Some(0),
                ..GetHolderResponse::default()
            })
        }
        tag @ (TAG_STATE | TAG_SCHEDULED_STATE_FINALIZED | TAG_SCHEDULED_STATE_CANCELLED) => {
            let status = match tag {
                TAG_STATE => Status::Active,
                TAG_SCHEDULED_STATE_FINALIZED => Status::ScheduledFinalized,
                TAG_SCHEDULED_STATE_CANCELLED => Status::ScheduledCanceled,
                _ => unreachable!(),
            };
            // StateAccount::from_account doesn't work here because state contains heap
            // and transaction inside state account has been allocated via this heap.
            // Data should be read by pointers with offsets.
            let (plain, accounts, tx_rlp) = StateAccount::get_state_account_view(program_id, info)?;
            let tx = Transaction::parse_from_rlp(tx_rlp.as_slice(), None)?;

            let tx_params = TxParams::from_transaction(plain.origin, &tx);

            Ok(GetHolderResponse {
                status,
                len: Some(data_len),
                owner: Some(plain.owner),
                tx: Some(tx.hash()),
                tx_data: Some(tx_params),
                tx_type: Some(tx.tx_type()),
                max_fee_per_gas: tx.max_fee_per_gas(),
                max_priority_fee_per_gas: tx.max_priority_fee_per_gas(),
                chain_id: tx.chain_id(),
                origin: Some(plain.origin),
                tree_account: plain.tree_account,
                block_params: Some(plain.block_params),
                accounts: Some(accounts),
                steps_executed: plain.steps_executed,
            })
        }
        _ => Err(ProgramError::InvalidAccountData.into()),
    }
}

pub async fn execute(
    rpc: &impl Rpc,
    program_id: &Pubkey,
    address: Pubkey,
) -> NeonResult<GetHolderResponse> {
    let response = rpc.get_account(&address).await?;
    let Some(mut account) = response else {
        return Ok(GetHolderResponse::empty());
    };

    let info = account_info(&address, &mut account);
    Ok(read_holder(program_id, &info).unwrap_or_else(GetHolderResponse::error))
}
