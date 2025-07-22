use std::marker::PhantomData;

use allocator_api2::alloc::{Allocator, Global};
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::clock::Clock;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;

use super::owned_account::OwnedAccountInfo;
use super::precompile_extension::{
    call_precompile_extension, is_precompile_extension, PrecompileDatabase,
};
use super::transient_storage::TransientStorage;
use super::{Action, BlockParams, IterativeActions};

use crate::account::{Account, AllocateResult};
use crate::config::STATIC_STORAGE_LIMIT;
use crate::error::{Error, Result};
use crate::evm::database::Database;
use crate::evm::precompile::is_precompile_address;
use crate::evm::Context;
use crate::executor::ActionExecutor;
use crate::platform::{InvokeMode, KeysIndex, Platform};
use crate::types::tree_map_cell::TreeMapCell;
use crate::types::Address;

pub type TimestampedContracts<A> = TreeMapCell<Address, (), A>;

#[repr(C)]
pub struct ExecutorStateData<A: Allocator> {
    pub inhereted_block_params: Option<BlockParams>,
    pub timestamped_contracts: TimestampedContracts<A>,
    pub actions: IterativeActions<A>,
    pub transient_storage: TransientStorage<A>,
}

impl ExecutorStateData<Global> {
    #[must_use]
    pub fn new() -> Self {
        Self::new_in(Global)
    }
}

impl<A: Allocator + Copy> ExecutorStateData<A> {
    #[must_use]
    pub fn new_in(allocator: A) -> Self {
        Self {
            inhereted_block_params: None,
            timestamped_contracts: TimestampedContracts::new_in(allocator),
            actions: IterativeActions::new_in(allocator),
            transient_storage: TransientStorage::new_in(allocator),
        }
    }

    #[must_use]
    pub fn with_block_params_in(clock: &Clock, allocator: A) -> Self {
        let block_params = BlockParams {
            number: U256::from(clock.slot),
            timestamp: U256::try_from(clock.unix_timestamp).expect("Timestamp is positive"),
        };
        block_params.log_data();

        Self {
            inhereted_block_params: Some(block_params),
            timestamped_contracts: TimestampedContracts::new_in(allocator),
            actions: IterativeActions::new_in(allocator),
            transient_storage: TransientStorage::new_in(allocator),
        }
    }
}

pub struct SyncedExecutorState<'r, 'a, A, P>
where
    A: Allocator,
    P: Platform<'a>,
{
    platform: &'r mut P,
    data: &'r mut ExecutorStateData<A>,
    allocator: A,
    phantom: PhantomData<&'a P>,
}

impl<'r, 'a, P> SyncedExecutorState<'r, 'a, Global, P>
where
    P: Platform<'a>,
{
    #[must_use]
    pub fn new(platform: &'r mut P, data: &'r mut ExecutorStateData<Global>) -> Self {
        Self::new_in(platform, data, Global)
    }
}

impl<'r, 'a, A, P> SyncedExecutorState<'r, 'a, A, P>
where
    A: Allocator + Copy,
    P: Platform<'a>,
{
    #[must_use]
    pub fn new_in(platform: &'r mut P, data: &'r mut ExecutorStateData<A>, allocator: A) -> Self {
        Self {
            platform,
            data,
            allocator,
            phantom: PhantomData,
        }
    }

    #[must_use]
    pub fn platform(&self) -> &P {
        self.platform
    }

    #[must_use]
    pub fn platform_mut(&mut self) -> &mut P {
        self.platform
    }

    pub fn actions(&self) -> &IterativeActions<A> {
        &self.data.actions
    }

    pub fn allocator(&self) -> A {
        self.allocator
    }

    pub fn use_timestamp_by(&self, address: Address) {
        self.data.timestamped_contracts.insert(address, ());
    }

    #[maybe_async]
    pub async fn allocate_state_in_solana(&mut self) -> Result<AllocateResult> {
        self.data.actions.allocate(self.platform).await
    }

    #[maybe_async]
    pub async fn commit_actions_to_solana(&mut self) -> Result<()> {
        self.data.actions.execute(self.platform).await
    }

    #[maybe_async]
    pub async fn commit_timestamps_to_solana(&mut self) -> Result<()> {
        let contracts_addresses = self.data.timestamped_contracts.drain().map(|v| v.0);

        for address in contracts_addresses {
            let mut contract = self
                .platform
                .get_contract(address)
                .await?
                .expect("Contract exists at this point");

            let clock = self.platform.get_sysvar::<Clock>().await?;
            contract.update_timestamp_used_at(&clock)?;
        }

        Ok(())
    }

    pub fn add_action(&mut self, action: Action<A>) {
        self.data.actions.push(action);
    }
}

#[maybe_async(?Send)]
impl<'a, A, P> Database for SyncedExecutorState<'_, 'a, A, P>
where
    A: Allocator + Copy,
    P: Platform<'a>,
{
    fn default_chain_id(&self) -> u64 {
        self.platform.default_chain()
    }

    async fn contract_chain_id(&self, address: Address) -> Result<u64> {
        if is_precompile_address(&address) || is_precompile_extension(&address) {
            return Ok(self.platform.default_chain());
        }

        let Some(contract) = self.platform.get_contract(address).await? else {
            return Ok(self.platform.default_chain());
        };

        Ok(contract.chain_id())
    }

    async fn log_event<const N: usize>(
        &mut self,
        address: Address,
        topics: [[u8; 32]; N],
        data: &[u8],
    ) -> Result<()> {
        self.platform.log_event(address, topics, data).await;
        Ok(())
    }

    async fn nonce(&self, address: Address, chain_id: u64) -> Result<u64> {
        if is_precompile_extension(&address) {
            return Ok(1_u64);
        }

        let Some(balance) = self.platform.get_balance(address, chain_id).await? else {
            return Ok(0_u64);
        };

        Ok(balance.nonce())
    }

    async fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()> {
        let mut balance = self.platform.create_balance(address, chain_id).await?;
        balance.increment_nonce()
    }

    async fn balance(&self, address: Address, chain_id: u64) -> Result<U256> {
        let Some(balance) = self.platform.get_balance(address, chain_id).await? else {
            return Ok(U256::ZERO);
        };

        Ok(balance.balance())
    }

    async fn transfer(
        &mut self,
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()> {
        if value == U256::ZERO {
            return Ok(());
        }

        let target_chain_id = self.contract_chain_id(target).await.unwrap_or(chain_id);

        if (self.code_size(target).await? > 0) && (target_chain_id != chain_id) {
            return Err(Error::InvalidTransferToken(source, chain_id));
        }

        let mut source = self.platform.create_balance(source, chain_id).await?;
        let mut target = self.platform.create_balance(target, chain_id).await?;

        source.transfer(&mut target, value)
    }

    async fn code_size(&self, address: Address) -> Result<usize> {
        if is_precompile_extension(&address) {
            return Ok(1_usize);
        }

        if is_precompile_address(&address) {
            // Check here, so we can remove precompile accounts from the transaction
            return Ok(0_usize);
        }

        let Some(contract) = self.platform.get_contract(address).await? else {
            return Ok(0_usize);
        };

        Ok(contract.code_len())
    }

    async fn use_code<R, F>(&self, address: Address, action: F) -> Result<R>
    where
        F: for<'code> FnOnce(&'code [u8]) -> R,
    {
        if is_precompile_extension(&address) {
            return Ok(action(&[0xFE]));
        }

        if is_precompile_address(&address) {
            return Ok(action(&[]));
        }

        let Some(contract) = self.platform.get_contract(address).await? else {
            return Ok(action(&[]));
        };

        let code = contract.code();
        Ok(action(&code))
    }

    async fn start_create(&mut self, address: Address, chain_id: u64) -> Result<()> {
        let _ = self.platform.create_contract(address, chain_id).await?;
        Ok(())
    }

    async fn end_create(&mut self, address: Address, code: &[u8]) -> Result<()> {
        if code.starts_with(&[0xEF]) {
            // https://eips.ethereum.org/EIPS/eip-3541
            return Err(Error::EVMObjectFormatNotSupported(address));
        }

        if code.len() > 0x6000 {
            // https://eips.ethereum.org/EIPS/eip-170
            return Err(Error::ContractCodeSizeLimit(address, code.len()));
        }

        let Some(mut contract) = self.platform.get_contract(address).await? else {
            unreachable!();
        };

        assert_eq!(contract.code_len(), 0);

        contract.allocate_entire_code_buffer(code)?;
        contract.set_code(code)
    }

    async fn storage(&self, address: Address, index: U256) -> Result<[u8; 32]> {
        if index < STATIC_STORAGE_LIMIT {
            let index: usize = index.as_usize();

            let Some(contract) = self.platform.get_contract(address).await? else {
                return Ok([0_u8; 32]);
            };

            Ok(contract.storage_value(index))
        } else {
            let subindex = (index & 0xFF).as_u8();
            let index = index & !U256::new(0xFF);

            let Some(storage) = self.platform.get_storage(address, index).await? else {
                return Ok([0_u8; 32]);
            };

            Ok(storage.get(subindex))
        }
    }

    async fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()> {
        if index < STATIC_STORAGE_LIMIT {
            let index: usize = index.as_usize();

            let Some(mut contract) = self.platform.get_contract(address).await? else {
                unreachable!();
            };

            contract.set_storage_value(index, &value)
        } else {
            let subindex = (index & 0xFF).as_u8();
            let index = index & !U256::new(0xFF);

            let mut storage = self.platform.create_storage(address, index).await?;
            storage.update(subindex, &value)
        }
    }

    async fn transient_storage(&self, address: Address, index: U256) -> Result<[u8; 32]> {
        let value = self.data.transient_storage.read(address, index);
        Ok(value)
    }

    fn set_transient_storage(
        &mut self,
        address: Address,
        index: U256,
        value: [u8; 32],
    ) -> Result<()> {
        self.data.transient_storage.write(address, index, value);
        Ok(())
    }

    async fn block_hash(&self, number: U256, context: &Context) -> Result<[u8; 32]> {
        if number >= U256::from(u64::MAX) {
            return Ok(<[u8; 32]>::default());
        }

        let current_slot = self.block_number(context).await?;
        let lower_slot = current_slot.saturating_sub(256_u64.into());

        if (number >= current_slot) || (lower_slot > number) {
            return Ok(<[u8; 32]>::default());
        }

        let number = number.as_u64();
        super::block_hash::find_slot_hash(number, self.platform).await
    }

    async fn block_number(&self, context: &Context) -> Result<U256> {
        self.use_timestamp_by(context.contract);

        if let Some(block_params) = &self.data.inhereted_block_params {
            return Ok(block_params.number);
        }

        let clock = self.platform.get_sysvar::<Clock>().await?;
        let slot = clock.slot.into();
        Ok(slot)
    }

    async fn block_timestamp(&self, context: &Context) -> Result<U256> {
        self.use_timestamp_by(context.contract);

        if let Some(block_params) = &self.data.inhereted_block_params {
            return Ok(block_params.timestamp);
        }

        let clock = self.platform.get_sysvar::<Clock>().await?;
        let timestamp = clock.unix_timestamp.try_into()?;
        Ok(timestamp)
    }

    async fn precompile_extension(
        &mut self,
        context: &Context,
        address: &Address,
        data: &[u8],
        is_static: bool,
    ) -> Option<Result<Vec<u8>>> {
        call_precompile_extension(self, context, address, data, is_static).await
    }

    fn snapshot(&mut self) {
        self.platform.snapshot();
        self.data.actions.snapshot();
        self.data.transient_storage.snapshot();
    }

    fn revert_snapshot(&mut self) {
        self.platform.revert();
        self.data.actions.revert();
        self.data.transient_storage.revert();
    }

    fn commit_snapshot(&mut self) {
        self.platform.commit();
        self.data.actions.commit();
        self.data.transient_storage.commit();
    }
}

#[maybe_async(?Send)]
impl<'a, A, P> PrecompileDatabase for SyncedExecutorState<'_, 'a, A, P>
where
    A: Allocator + Copy,
    P: Platform<'a>,
{
    fn program_id(&self) -> Pubkey {
        self.platform.program_id()
    }

    fn operator(&self) -> Pubkey {
        self.platform.operator()
    }

    fn chains(&self) -> impl Iterator<Item = crate::platform::Chain> {
        self.platform.chains()
    }

    fn keys(&self) -> &impl KeysIndex {
        self.platform.keys()
    }

    async fn use_real_account<R, F>(&self, pubkey: Pubkey, f: F) -> Result<R>
    where
        F: FnOnce(&Account) -> R,
    {
        let account = self.platform.get_real_account(pubkey).await?;
        Ok(f(&account))
    }

    async fn rent(&self) -> Result<Rent> {
        self.platform.get_sysvar::<Rent>().await
    }

    fn is_iterative_mode(&self) -> bool {
        false
    }

    async fn invoke(&mut self, instruction: Instruction, seeds: &[&[&[u8]]]) -> Result<()> {
        self.platform
            .invoke(instruction, seeds, InvokeMode::Normal)
            .await
    }

    async fn queue_invoke(&mut self, instruction: Instruction, seeds: &[&[&[u8]]]) -> Result<()> {
        self.platform
            .invoke(instruction, seeds, InvokeMode::Queued)
            .await
    }

    async fn external_account(&self, pubkey: Pubkey) -> Result<OwnedAccountInfo> {
        let account = self.platform.get_real_account(pubkey).await?;
        Ok(OwnedAccountInfo::from_account(self.program_id(), &account))
    }

    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        self.platform.get_return_data()
    }

    async fn solana_user_pubkey(&self, address: Address) -> Result<Option<Pubkey>> {
        let Some(sol_chain) = self.chains().find(|c| c.name == "sol") else {
            return Ok(None);
        };

        let pubkey = self
            .platform
            .get_balance(address, sol_chain.id)
            .await?
            .map(|b| b.solana_address())
            .unwrap_or_default();

        Ok(pubkey)
    }

    async fn burn(&mut self, address: Address, chain_id: u64, amount: U256) -> Result<()> {
        if amount == U256::ZERO {
            return Ok(());
        }

        let mut balance = self.platform.create_balance(address, chain_id).await?;
        balance.burn(amount)
    }
}
