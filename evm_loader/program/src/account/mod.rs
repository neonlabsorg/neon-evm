use crate::error::Result;
use solana_program::account_info::AccountInfo;

pub use abstraction::{
    AbstractAccount, Account, AccountHeader, AccountRead, AccountWrite, NoHeader,
    ACCOUNT_PREFIX_LEN,
};
pub use container::{AccountInContainer, Container, Reference};
pub use ether_balance::{Balance, Header as BalanceHeader};
pub use ether_contract::{AllocateResult, Contract, Header as ContractHeader};
pub use ether_storage::{Cell, StorageCell, StorageCellSeed};
pub use holder::{Header as HolderHeader, Holder};
pub use operator::Operator;
pub use operator_balance::{OperatorBalance, OperatorBalanceValidator};
pub use state::StateAccount;
pub use state_finalized::{Header as StateFinalizedHeader, StateFinalizedAccount};
pub use state_root::{InterruptedState, Root};
pub use transaction_tree::{
    NodeInitializer, Status as TransactionTreeNodeStatus, TransactionTree, TreeInitializer,
    NO_CHILD_TRANSACTION,
};
pub use treasury::{MainTreasury, Treasury};

mod abstraction;
mod container;
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
mod state_root;
pub mod token;
mod transaction_tree;
mod treasury;

pub const TAG_EMPTY: u8 = 0;
pub const TAG_STATE: u8 = 26;
pub const TAG_STATE_FINALIZED: u8 = 32;
pub const TAG_SCHEDULED_STATE_FINALIZED: u8 = 36;
pub const TAG_SCHEDULED_STATE_CANCELLED: u8 = 39;
pub const TAG_HOLDER: u8 = 52;

pub const TAG_ACCOUNT_BALANCE: u8 = 60;
pub const TAG_ACCOUNT_CONTRACT: u8 = 70;
pub const TAG_OPERATOR_BALANCE: u8 = 80;
pub const TAG_STORAGE_CELL: u8 = 43;
pub const TAG_TRANSACTION_TREE: u8 = 90;

pub const TAG_CONTAINER: u8 = 100;
pub const TAG_REFERENCE: u8 = 110;

/// # Safety
/// *Permanently delete all data* in the account. Transfer lamports to the operator.
pub unsafe fn delete(account: &AccountInfo, operator: &Operator) -> Result<()> {
    debug_print!("DELETE ACCOUNT {}", account.key);

    **operator.lamports.borrow_mut() += account.lamports();
    **account.lamports.borrow_mut() = 0;

    account.data.borrow_mut().fill(0);
    account.resize(0)?;
    account.assign(&solana_program::system_program::ID);

    Ok(())
}

/// # Safety
/// *Permanently delete all data* in the account. Transfer lamports to the treasury.
pub unsafe fn delete_with_treasury(account: &AccountInfo, treasury: &Treasury) -> Result<()> {
    debug_print!("DELETE ACCOUNT {}", account.key);

    **treasury.lamports.borrow_mut() += account.lamports();
    **account.lamports.borrow_mut() = 0;

    account.data.borrow_mut().fill(0);
    account.resize(0)?;
    account.assign(&solana_program::system_program::ID);

    Ok(())
}
