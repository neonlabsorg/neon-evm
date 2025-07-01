#![allow(unused_mut)] // TODO remove after state.rs fix
#![allow(clippy::needless_pass_by_ref_mut)] // TODO remove after state.rs fix

use crate::error::{Error, Result};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

pub use crate::{account_storage::FAKE_OPERATOR, config::ACCOUNT_SEED_VERSION};

pub use abstraction::{
    Account, AccountDispatch, AccountHeader, NoHeader, ZeroInit, ACCOUNT_PREFIX_LEN,
};
pub use ether_balance::{BalanceAccount, Header as BalanceHeader};
pub use ether_contract::{AllocateResult, ContractAccount, Header as ContractHeader};
pub use ether_storage::{Cell, Header as StorageCellHeader, StorageCell, StorageCellAddress};
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

use self::program::System;

mod abstraction;
mod ether_balance;
mod ether_contract;
mod ether_storage;
mod holder;
mod operator;
mod operator_balance;
pub mod pda_accounts;
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

pub struct AccountsDB<'a> {
    sorted_accounts: Vec<AccountInfo<'a>>,
    operator: Operator<'a>,
    operator_balance: Option<OperatorBalance<'a>>,
    system: Option<System<'a>>,
    treasury: Option<Treasury<'a>>,
}

impl<'a> AccountsDB<'a> {
    #[must_use]
    pub fn new(
        accounts: &[AccountInfo<'a>],
        operator: Operator<'a>,
        operator_balance: Option<OperatorBalance<'a>>,
        system: Option<System<'a>>,
        treasury: Option<Treasury<'a>>,
    ) -> Self {
        let mut sorted_accounts = accounts.to_vec();
        sorted_accounts.sort_unstable_by_key(|a| a.key);
        sorted_accounts.dedup_by_key(|a| a.key);

        Self {
            sorted_accounts,
            operator,
            operator_balance,
            system,
            treasury,
        }
    }

    #[must_use]
    pub fn accounts_len(&self) -> usize {
        self.sorted_accounts.len()
    }

    #[must_use]
    pub fn system(&self) -> &System<'a> {
        if let Some(system) = &self.system {
            return system;
        }

        panic_with_error!(Error::AccountMissing(solana_program::system_program::ID));
    }

    #[must_use]
    pub fn treasury(&self) -> &Treasury<'a> {
        if let Some(treasury) = &self.treasury {
            return treasury;
        }

        panic_with_error!(Error::TreasuryMissing);
    }

    #[must_use]
    pub fn has_treasury(&self) -> bool {
        self.treasury.is_some()
    }

    #[must_use]
    pub fn operator(&self) -> &Operator<'a> {
        &self.operator
    }

    #[must_use]
    pub fn operator_balance(&self) -> OperatorBalance<'a> {
        if let Some(operator_balance) = &self.operator_balance {
            return operator_balance.clone();
        }

        panic_with_error!(Error::OperatorBalanceMissing);
    }

    #[must_use]
    pub fn try_operator_balance(&self) -> Option<OperatorBalance<'a>> {
        self.operator_balance.clone()
    }

    #[must_use]
    pub fn operator_key(&self) -> Pubkey {
        *self.operator.key
    }

    #[must_use]
    pub fn operator_info(&self) -> &AccountInfo<'a> {
        &self.operator
    }

    #[must_use]
    pub fn get(&self, pubkey: &Pubkey) -> &AccountInfo<'a> {
        if pubkey == &FAKE_OPERATOR || pubkey == self.operator.key {
            return self.operator_info();
        }
        let index = self
            .sorted_accounts
            .binary_search_by_key(&pubkey, |a| a.key)
            .unwrap_or_else(|_| panic_with_error!(Error::AccountMissing(*pubkey)));

        // We just got an 'index' from the binary_search over this vector.
        unsafe { self.sorted_accounts.get_unchecked(index) }
    }
}

#[allow(clippy::into_iter_without_iter)]
impl<'a, 'r> IntoIterator for &'r AccountsDB<'a> {
    type Item = &'r AccountInfo<'a>;
    type IntoIter = std::slice::Iter<'r, AccountInfo<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.sorted_accounts.iter()
    }
}
