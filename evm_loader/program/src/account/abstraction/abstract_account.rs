use solana_program::pubkey::Pubkey;

use crate::account::{Account, AccountInContainer, AccountRead, AccountWrite};

pub enum AbstractAccount<T> {
    RawAccount(T),
    AccountInContainer(AccountInContainer<T>),
}

impl<T> AbstractAccount<T> {
    #[must_use]
    pub const fn as_raw_account(&self) -> &T {
        let AbstractAccount::RawAccount(raw_account) = self else {
            panic!("Account expected to be not in container");
        };

        raw_account
    }
}

impl<T> From<T> for AbstractAccount<T> {
    fn from(account: T) -> Self {
        AbstractAccount::RawAccount(account)
    }
}

impl<T> From<AccountInContainer<T>> for AbstractAccount<T> {
    fn from(account: AccountInContainer<T>) -> Self {
        AbstractAccount::AccountInContainer(account)
    }
}

impl<T: AccountRead> AccountRead for AbstractAccount<T> {
    fn data(&self) -> std::cell::Ref<[u8]> {
        match self {
            AbstractAccount::RawAccount(account) => account.data(),
            AbstractAccount::AccountInContainer(account) => account.data(),
        }
    }

    fn data_len(&self) -> usize {
        match self {
            AbstractAccount::RawAccount(account) => account.data_len(),
            AbstractAccount::AccountInContainer(account) => account.data_len(),
        }
    }

    fn original_data_len(&self) -> usize {
        match self {
            AbstractAccount::RawAccount(account) => account.original_data_len(),
            AbstractAccount::AccountInContainer(account) => account.original_data_len(),
        }
    }

    fn pubkey(&self) -> Pubkey {
        match self {
            AbstractAccount::RawAccount(account) => account.pubkey(),
            AbstractAccount::AccountInContainer(account) => account.pubkey(),
        }
    }

    fn container(&self) -> Option<Pubkey> {
        match self {
            AbstractAccount::RawAccount(account) => account.container(),
            AbstractAccount::AccountInContainer(account) => account.container(),
        }
    }

    fn owner(&self) -> Pubkey {
        match self {
            AbstractAccount::RawAccount(account) => account.owner(),
            AbstractAccount::AccountInContainer(account) => account.owner(),
        }
    }

    fn is_system_owned(&self) -> bool {
        match self {
            AbstractAccount::RawAccount(account) => account.is_system_owned(),
            AbstractAccount::AccountInContainer(account) => account.is_system_owned(),
        }
    }

    fn lamports(&self) -> u64 {
        match self {
            AbstractAccount::RawAccount(account) => account.lamports(),
            AbstractAccount::AccountInContainer(account) => account.lamports(),
        }
    }

    fn rent_epoch(&self) -> u64 {
        match self {
            AbstractAccount::RawAccount(account) => account.rent_epoch(),
            AbstractAccount::AccountInContainer(account) => account.rent_epoch(),
        }
    }

    fn is_executable(&self) -> bool {
        match self {
            AbstractAccount::RawAccount(account) => account.is_executable(),
            AbstractAccount::AccountInContainer(account) => account.is_executable(),
        }
    }
}

impl<T: Account> AccountWrite for AbstractAccount<T> {
    fn data_mut(&mut self) -> std::cell::RefMut<[u8]> {
        match self {
            AbstractAccount::RawAccount(account) => account.data_mut(),
            AbstractAccount::AccountInContainer(account) => account.data_mut(),
        }
    }

    fn reallocate(&mut self, new_size: usize) -> crate::error::Result<()> {
        match self {
            AbstractAccount::RawAccount(account) => account.reallocate(new_size),
            AbstractAccount::AccountInContainer(account) => account.reallocate(new_size),
        }
    }
}

impl<T: Account> Account for AbstractAccount<T> {}
