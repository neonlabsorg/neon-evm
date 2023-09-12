use crate::account::{
    AccountsDB, BalanceAccount, ContractAccount, Operator, StorageCell, Treasury,
};
use crate::account_storage::ProgramAccountStorage;
use crate::error::Result;
use crate::types::Address;
use ethnum::U256;
use solana_program::clock::Clock;
use solana_program::sysvar::Sysvar;

use super::keys_cache::KeysCache;

impl<'a> ProgramAccountStorage<'a> {
    pub fn new(accounts: AccountsDB<'a>) -> Result<Self> {
        Ok(Self {
            clock: Clock::get()?,
            accounts,
            keys: KeysCache::new(),
        })
    }

    pub fn operator(&self) -> &Operator<'a> {
        self.accounts.operator()
    }

    pub fn treasury(&self) -> &Treasury<'a> {
        self.accounts.treasury()
    }

    pub fn db(&self) -> &AccountsDB<'a> {
        &self.accounts
    }

    pub fn storage_cell(&self, address: Address, index: U256) -> Result<StorageCell<'a>> {
        let pubkey = self.keys.storage_cell(&crate::ID, address, index);

        let account = self.accounts.get(&pubkey);
        StorageCell::from_account(&crate::ID, account.clone())
    }

    pub fn contract_account(&self, address: Address) -> Result<ContractAccount<'a>> {
        let pubkey = self.keys.contract(&crate::ID, address);

        let account = self.accounts.get(&pubkey);
        ContractAccount::from_account(&crate::ID, account.clone())
    }

    pub fn balance_account(&self, address: Address, chain_id: u64) -> Result<BalanceAccount<'a>> {
        let pubkey = self.keys.balance(&crate::ID, address, chain_id);

        let account = self.accounts.get(&pubkey);
        BalanceAccount::from_account(&crate::ID, account.clone(), Some(address))
    }

    pub fn create_balance_account(
        &self,
        address: Address,
        chain_id: u64,
    ) -> Result<BalanceAccount<'a>> {
        BalanceAccount::create(address, chain_id, &self.accounts)
    }
}
