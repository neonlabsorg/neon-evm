use allocator_api2::alloc::Allocator;
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::pubkey;
use solana_program::{
    entrypoint::MAX_PERMITTED_DATA_INCREASE, instruction::Instruction, pubkey::Pubkey,
    sysvar::Sysvar,
};

use crate::account::{
    pda, Account, AccountDispatch, AllocateResult, BalanceAccount, ContractAccount, Root,
    StorageCell, StorageCellSeed, ZeroInit, TAG_EMPTY,
};
use crate::error::Result;
use crate::types::{Address, Transaction};

mod keys_index;
pub use keys_index::{CachedKeysIndex, DefaultKeysIndex, KeysIndex};

#[cfg(target_os = "solana")]
mod solana;
#[cfg(target_os = "solana")]
pub use solana::Solana;

pub const FAKE_OPERATOR: Pubkey = pubkey!("neonoperator1111111111111111111111111111111");

pub struct Chain {
    pub id: u64,
    pub name: String,
    pub token: Pubkey,
}

pub trait OriginId {
    fn address(&self) -> Address;
    fn chain_id(&self) -> Option<u64>;
}
impl OriginId for (Address, &dyn Transaction) {
    fn address(&self) -> Address {
        self.0
    }

    fn chain_id(&self) -> Option<u64> {
        self.1.chain_id()
    }
}
impl<A: Allocator + Copy> OriginId for &Root<A> {
    fn address(&self) -> Address {
        self.origin()
    }

    fn chain_id(&self) -> Option<u64> {
        self.tx_chain_id()
    }
}

#[derive(PartialEq, Eq)]
pub enum InvokeMode {
    Normal,
    Queued,
}

#[maybe_async(?Send)]
pub trait Platform<'a>: Sized {
    fn program_id(&self) -> Pubkey;
    fn operator(&self) -> Pubkey;

    fn chains(&self) -> impl Iterator<Item = Chain>;
    fn default_chain(&self) -> u64;

    fn keys(&self) -> &impl KeysIndex;

    async fn invoke(
        &mut self,
        instruction: Instruction,
        seeds: &[&[&[u8]]],
        mode: InvokeMode,
    ) -> Result<()>;
    fn get_return_data(&self) -> Option<(Pubkey, Vec<u8>)>;

    async fn log_event<const N: usize>(
        &mut self,
        address: Address,
        topics: [[u8; 32]; N],
        data: &[u8],
    );

    async fn get_sysvar<T: Sysvar>(&self) -> Result<T>;
    async fn get_sysvar_part<T: Sysvar>(&self, offset: usize, buffer: &mut [u8]) -> Result<()>;
    async fn get_account(&self, pubkey: Pubkey) -> Result<Account<'a>>;
    async fn get_real_account(&self, pubkey: Pubkey) -> Result<Account<'a>>;

    async fn assign_account(&mut self, seeds: &[&[u8]]) -> Result<Account<'a>>;
    async fn assign_account_with_seed(
        &mut self,
        base: Pubkey,
        seed: &str,
        base_seeds: &[&[u8]],
    ) -> Result<Account<'a>>;

    fn snapshot(&mut self);
    fn revert(&mut self);
    fn commit(&mut self);

    async fn get_origin(&mut self, id: impl OriginId) -> Result<BalanceAccount<'a>> {
        let chain_id = id.chain_id().unwrap_or_else(|| self.default_chain());
        self.create_balance(id.address(), chain_id).await
    }

    async fn get_balance(
        &self,
        address: Address,
        chain_id: u64,
    ) -> Result<Option<BalanceAccount<'a>>> {
        let program_id = self.program_id();

        let pubkey = self.keys().balance(address, chain_id);

        let account = self.get_account(pubkey).await?;
        if account.is_system_owned() {
            return Ok(None);
        }

        let balance = BalanceAccount::from_account(program_id, account)?;
        Ok(Some(balance))
    }

    async fn create_balance(
        &mut self,
        address: Address,
        chain_id: u64,
    ) -> Result<BalanceAccount<'a>> {
        let program_id = self.program_id();

        let (_, bump_seed) = self.keys().balance_bump(address, chain_id);
        let seeds: &[&[u8]] = pda::balance_seeds!(address, chain_id, bump_seed);

        let mut account = self.assign_account(seeds).await?;
        if account.data_len() == 0 {
            let required_len = BalanceAccount::required_account_size(false);
            account.reallocate(required_len, ZeroInit::Uninit)?;

            BalanceAccount::initialize(account, program_id, address, chain_id)
        } else {
            BalanceAccount::from_account(program_id, account)
        }
    }

    async fn create_balance_for_solana_user(
        &mut self,
        user_pubkey: Pubkey,
    ) -> Result<BalanceAccount<'a>> {
        let program_id = self.program_id();

        let address = Address::from_solana_address(&user_pubkey);
        let chain = self.chains().find(|c| c.name == "sol").unwrap();

        let (_, bump_seed) = self.keys().balance_bump(address, chain.id);
        let seeds: &[&[u8]] = pda::balance_seeds!(address, chain.id, bump_seed);

        let mut account = self.assign_account(seeds).await?;
        if account.data_len() == 0 {
            let required_len = BalanceAccount::required_account_size(true);
            account.reallocate(required_len, ZeroInit::Uninit)?;

            BalanceAccount::initialize_for_solana_user(account, program_id, user_pubkey, chain.id)
        } else {
            let mut balance = BalanceAccount::from_account(program_id, account)?;
            if balance.solana_address().is_none() {
                balance.set_solana_address(user_pubkey)?;
            }

            Ok(balance)
        }
    }

    async fn get_contract(&self, address: Address) -> Result<Option<ContractAccount<'a>>> {
        let program_id = self.program_id();

        let pubkey = self.keys().contract(address);

        let account = self.get_account(pubkey).await?;
        if account.is_system_owned() || (account.tag(program_id)? == TAG_EMPTY) {
            return Ok(None);
        }

        let contract = ContractAccount::from_account(program_id, account)?;
        Ok(Some(contract))
    }

    async fn create_contract(
        &mut self,
        address: Address,
        chain_id: u64,
    ) -> Result<ContractAccount<'a>> {
        let program_id = self.program_id();

        let (_, bump_seed) = self.keys().contract_bump(address);
        let seeds: &[&[u8]] = pda::contract_seeds!(address, bump_seed);

        let mut account = self.assign_account(seeds).await?;
        if account.data_len() == 0 {
            let required_len = ContractAccount::required_account_size(&[]);
            account.reallocate(required_len, ZeroInit::Uninit)?;

            ContractAccount::initialize(account, program_id, address, chain_id, &[])
        } else {
            let contract = ContractAccount::from_account(program_id, account)?;
            assert_eq!(contract.address(), address);
            assert_eq!(contract.chain_id(), chain_id);

            Ok(contract)
        }
    }

    async fn allocate_contract(&mut self, address: Address, code: &[u8]) -> Result<AllocateResult> {
        let program_id = self.program_id();

        let (_, bump_seed) = self.keys().contract_bump(address);
        let seeds: &[&[u8]] = pda::contract_seeds!(address, bump_seed);

        let mut account = self.assign_account(seeds).await?;
        assert!((account.data_len() == 0) || account.validate_tag(program_id, TAG_EMPTY).is_ok());

        let required_size = ContractAccount::required_account_size(code);
        if account.data_len() >= required_size {
            return Ok(AllocateResult::Ready);
        }

        let max_size = account.original_data_len() + MAX_PERMITTED_DATA_INCREASE;
        let new_space = required_size.min(max_size);
        account.reallocate(new_space, ZeroInit::Uninit)?;

        if new_space >= required_size {
            Ok(AllocateResult::Ready)
        } else {
            Ok(AllocateResult::NeedMore)
        }
    }

    async fn initialize_allocated_contract(
        &mut self,
        address: Address,
        chain_id: u64,
        code: &[u8],
    ) -> Result<ContractAccount<'a>> {
        let program_id = self.program_id();

        let pubkey = self.keys().contract(address);

        let account = self.get_account(pubkey).await?;
        ContractAccount::initialize(account, program_id, address, chain_id, code)
    }

    async fn get_storage(&self, contract: Address, index: U256) -> Result<Option<StorageCell<'a>>> {
        let program_id = self.program_id();

        let pubkey = self.keys().storage(contract, index);

        let account = self.get_account(pubkey).await?;
        if account.is_system_owned() {
            return Ok(None);
        }

        let storage_cell = StorageCell::from_account(program_id, account)?;
        Ok(Some(storage_cell))
    }

    async fn create_storage(&mut self, contract: Address, index: U256) -> Result<StorageCell<'a>> {
        let program_id = self.program_id();

        let (base, bump_seed) = self.keys().contract_bump(contract);
        let base_seeds: &[&[u8]] = pda::contract_seeds!(contract, bump_seed);

        let storage_seed = StorageCellSeed::new(index);

        let mut account = self
            .assign_account_with_seed(base, &storage_seed, base_seeds)
            .await?;

        if account.data_len() == 0 {
            let required_len = StorageCell::required_account_size(0);
            account.reallocate(required_len, ZeroInit::Uninit)?;

            StorageCell::initialize(account, program_id)
        } else {
            StorageCell::from_account(program_id, account)
        }
    }
}
