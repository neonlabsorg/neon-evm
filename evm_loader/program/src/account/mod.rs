#![allow(unused_mut)] // TODO remove after state.rs fix
#![allow(clippy::needless_pass_by_ref_mut)] // TODO remove after state.rs fix

use crate::error::Result;
use solana_program::account_info::AccountInfo;

pub use abstraction::{
    Account, AccountDispatch, AccountHeader, NoHeader, ZeroInit, ACCOUNT_PREFIX_LEN,
};
pub use ether_balance::{BalanceAccount, Header as BalanceHeader};
pub use ether_contract::{AllocateResult, ContractAccount, Header as ContractHeader};
pub use ether_storage::{StorageCell, StorageCellSeed};
pub use holder::{Header as HolderHeader, Holder};
pub use operator::Operator;
pub use operator_balance::{OperatorBalance, OperatorBalanceValidator};
pub use state::{
    AccountsStatus, InterruptedInstruction, InterruptedState, PlainData as PlainStateHeader,
    StateAccount,
};
pub use state_finalized::{Header as StateFinalizedHeader, StateFinalizedAccount};
pub use transaction_tree::{
    NodeInitializer, Status as TransactionTreeNodeStatus, TransactionTree, TreeInitializer,
    NO_CHILD_TRANSACTION,
};
pub use treasury::{MainTreasury, Treasury};

mod abstraction;
mod ether_balance;
mod ether_contract;
mod ether_storage;
mod holder;
mod operator;
mod operator_balance;
pub mod pda;
pub mod program;
mod state;
mod state_finalized;
pub mod token;
mod transaction_tree;
mod treasury;

pub const HEAP_OFFSET_PTR: usize = holder::HEAP_OFFSET_OFFSET;

pub const TAG_EMPTY: u8 = 0;
pub const TAG_STATE: u8 = 25;
pub const TAG_STATE_FINALIZED: u8 = 32;
pub const TAG_SCHEDULED_STATE_FINALIZED: u8 = 35;
pub const TAG_SCHEDULED_STATE_CANCELLED: u8 = 38;
pub const TAG_HOLDER: u8 = 52;

pub const TAG_ACCOUNT_BALANCE: u8 = 60;
pub const TAG_ACCOUNT_CONTRACT: u8 = 70;
pub const TAG_OPERATOR_BALANCE: u8 = 80;
pub const TAG_STORAGE_CELL: u8 = 43;
pub const TAG_TRANSACTION_TREE: u8 = 90;

/// # Safety
/// *Permanently delete all data* in the account. Transfer lamports to the operator.
pub unsafe fn delete(account: &AccountInfo, operator: &Operator) {
    debug_print!("DELETE ACCOUNT {}", account.key);

    **operator.lamports.borrow_mut() += account.lamports();
    **account.lamports.borrow_mut() = 0;

    let mut data = account.data.borrow_mut();
    data.fill(0);
}

/// # Safety
/// *Permanently delete all data* in the account. Transfer lamports to the treasury.
pub unsafe fn delete_with_treasury(account: &AccountInfo, treasury: &Treasury) -> Result<()> {
    debug_print!("DELETE ACCOUNT {}", account.key);

    **treasury.lamports.borrow_mut() += account.lamports();
    **account.lamports.borrow_mut() = 0;

    account.data.borrow_mut().fill(0);
    account.realloc(0, false)?;
    account.assign(&solana_program::system_program::ID);

    Ok(())
}
