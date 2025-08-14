use std::{
    cell::{Cell, Ref, RefCell, RefMut},
    rc::Rc,
};

use evm_loader::account::{Account, AccountRead, AccountWrite};
use evm_loader::error::Result;
use solana_account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;

#[derive(Clone)]
pub struct SharedAccount {
    key: Pubkey,
    original_data_len: usize,
    backup: Rc<solana_account::Account>,
    account: Rc<RefCell<solana_account::Account>>,
    modified: Rc<Cell<bool>>,
}

impl SharedAccount {
    #[must_use]
    pub fn new(pubkey: Pubkey, account: &impl ReadableAccount) -> Self {
        let account: solana_account::Account = account.to_account_shared_data().into();

        Self {
            key: pubkey,
            original_data_len: account.data.len(),
            backup: Rc::new(account.clone()),
            account: Rc::new(RefCell::new(account)),
            modified: Rc::new(Cell::new(false)),
        }
    }

    #[must_use]
    pub fn new_empty(pubkey: Pubkey) -> Self {
        let empty_account = solana_account::Account::default();
        Self::new(pubkey, &empty_account)
    }

    #[must_use]
    pub fn deep_clone(&self) -> Self {
        let account = self.account.borrow().clone();

        Self {
            key: self.key,
            original_data_len: self.original_data_len,
            backup: Rc::new(account.clone()),
            account: Rc::new(RefCell::new(account)),
            modified: Rc::clone(&self.modified), // modified once, modified everywhere
        }
    }

    pub fn mark_modified(&self) {
        self.modified.set(true);
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.modified.get()
    }

    pub fn assign(&self, owner: Pubkey) {
        let mut account = self.account.borrow_mut();
        account.owner = owner;
    }

    pub fn update(&self, other: &impl ReadableAccount) {
        let mut account = self.account.borrow_mut();
        account.owner = *other.owner();
        account.data = other.data().to_vec();
        account.lamports = other.lamports();
        account.executable = other.executable();
    }

    pub fn revert(&self) {
        let backup = solana_account::Account::clone(&self.backup);
        self.account.replace(backup);
    }
}

impl From<&SharedAccount> for solana_account::Account {
    fn from(account: &SharedAccount) -> Self {
        Self {
            lamports: account.lamports(),
            data: account.data().to_vec(),
            owner: account.owner(),
            executable: account.is_executable(),
            rent_epoch: account.rent_epoch(),
        }
    }
}

impl AccountRead for SharedAccount {
    fn data(&self) -> Ref<[u8]> {
        let account = self.account.borrow();
        Ref::map(account, |a| a.data.as_slice())
    }

    fn data_len(&self) -> usize {
        let account = self.account.borrow();
        account.data.len()
    }

    fn original_data_len(&self) -> usize {
        self.original_data_len
    }

    fn pubkey(&self) -> Pubkey {
        self.key
    }

    fn container(&self) -> Option<Pubkey> {
        None
    }

    fn owner(&self) -> Pubkey {
        let account = self.account.borrow();
        account.owner
    }

    fn is_system_owned(&self) -> bool {
        let account = self.account.borrow();
        solana_sdk_ids::system_program::check_id(&account.owner)
    }

    fn lamports(&self) -> u64 {
        let account = self.account.borrow();
        account.lamports
    }

    fn rent_epoch(&self) -> u64 {
        let account = self.account.borrow();
        account.rent_epoch
    }

    fn is_executable(&self) -> bool {
        let account = self.account.borrow();
        account.executable
    }
}

impl AccountWrite for SharedAccount {
    fn data_mut(&mut self) -> RefMut<[u8]> {
        self.modified.set(true);

        let account = self.account.borrow_mut();
        RefMut::map(account, |a| a.data.as_mut_slice())
    }

    fn reallocate(&mut self, new_size: usize) -> Result<()> {
        self.modified.set(true);

        let mut account = self.account.borrow_mut();
        account.data.resize(new_size, 0); // No limits on SharedAccount reallocation

        Ok(())
    }
}

impl Account for SharedAccount {}
