use crate::account::AllocateResult;
use crate::error::{Error, Result};
use crate::evm::database::Database;
use crate::platform::{Chain, KeysIndex, Platform, FAKE_OPERATOR};
use crate::types::seeds::InvokeSeeds;
use crate::types::vector::{vector_map, VectorMap, VectorSliceExt, VectorVecExt, VectorVecSlowExt};
use crate::types::Address;
use allocator_api2::alloc::{self, Allocator};
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;

use super::precompile_extension::{call_precompile_extension, PrecompileDatabase};
use super::{
    action::Action, owned_account::OwnedAccountInfo, touched_accounts::TouchedAccounts,
    ExecutorStateData, IterativeActions, SyncedExecutorState,
};

pub struct ExecutorState<'r, A, P>
where
    A: Allocator,
    P: Platform,
{
    state: SyncedExecutorState<'r, A, P>,
    touched_accounts: TouchedAccounts,
}

#[maybe_async]
impl<'r, A, P> ExecutorState<'r, A, P>
where
    A: Allocator + Copy,
    P: Platform,
{
    #[must_use]
    pub fn new_in(platform: &'r mut P, data: &'r mut ExecutorStateData<A>, allocator: A) -> Self {
        Self {
            state: SyncedExecutorState::new_in(platform, data, allocator),
            touched_accounts: TouchedAccounts::new_in(alloc::Global), // Use global allocator since we don't need to store it between iterations
        }
    }

    pub fn allocator(&self) -> A {
        self.state.allocator()
    }

    pub fn add_action(&mut self, action: Action<A>) {
        self.state.add_action(action);
    }

    pub fn actions(&self) -> &IterativeActions<A> {
        self.state.actions()
    }

    #[maybe_async]
    pub async fn allocate_state_in_solana(&mut self) -> Result<AllocateResult> {
        self.state.allocate_state_in_solana().await
    }

    #[maybe_async]
    pub async fn commit_state_to_solana(&mut self) -> Result<()> {
        self.state.commit_actions_to_solana().await?;
        self.state.commit_timestamps_to_solana().await
    }

    async fn balance_indirect(&self, address: Address, chain_id: u64) -> Result<U256> {
        self.touched_accounts
            .touch_balance_indirect(address, chain_id);

        let actions = self.actions();

        let mut balance = self.state.balance(address, chain_id).await?;
        actions.apply_to_balance(address, chain_id, &mut balance);

        Ok(balance)
    }

    pub fn into_touched_accounts(self) -> TouchedAccounts {
        self.touched_accounts
    }
}

#[maybe_async(?Send)]
impl<A, P> Database for ExecutorState<'_, A, P>
where
    A: Allocator + Copy,
    P: Platform,
{
    fn default_chain_id(&self) -> u64 {
        self.state.default_chain_id()
    }

    async fn contract_chain_id(&self, address: Address) -> Result<u64> {
        self.touched_accounts.touch_contract(address);

        let Some(chain_id) = self.actions().find_contract_chain_id(address) else {
            return self.state.contract_chain_id(address).await;
        };

        Ok(chain_id)
    }

    async fn log_event<const N: usize>(
        &mut self,
        address: Address,
        topics: [[u8; 32]; N],
        data: &[u8],
    ) -> Result<()> {
        self.state.log_event(address, topics, data).await
    }

    async fn nonce(&self, address: Address, chain_id: u64) -> Result<u64> {
        let mut nonce = self.state.nonce(address, chain_id).await?;
        self.actions().apply_to_nonce(address, chain_id, &mut nonce);

        Ok(nonce)
    }

    async fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()> {
        let action = Action::EvmIncrementNonce { address, chain_id };
        self.add_action(action);

        Ok(())
    }

    async fn balance(&self, address: Address, chain_id: u64) -> Result<U256> {
        self.touched_accounts.touch_balance(address, chain_id);

        let actions = self.actions();

        let mut balance = self.state.balance(address, chain_id).await?;
        actions.apply_to_balance(address, chain_id, &mut balance);

        Ok(balance)
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

        if source == target {
            return Ok(());
        }

        let source_balance = self.balance_indirect(source, chain_id).await?;
        if source_balance < value {
            return Err(Error::InsufficientBalance(source, chain_id, value));
        }

        let action = Action::Transfer {
            source,
            target,
            chain_id,
            value,
        };
        self.add_action(action);

        Ok(())
    }

    async fn code_size(&self, address: Address) -> Result<usize> {
        self.touched_accounts.touch_contract(address);

        let Some(code) = self.actions().find_code(address) else {
            return self.state.code_size(address).await;
        };

        Ok(code.len())
    }

    async fn use_code<R, F>(&self, address: Address, action: F) -> Result<R>
    where
        F: for<'code> FnOnce(&'code [u8]) -> R,
    {
        self.touched_accounts.touch_contract(address);

        let Some(code) = self.actions().find_code(address) else {
            return self.state.use_code(address, action).await;
        };

        Ok(action(code))
    }

    async fn start_create(&mut self, address: Address, chain_id: u64) -> Result<()> {
        let action = Action::EvmStartCreate { address, chain_id };
        self.add_action(action);

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

        let action = Action::EvmEndCreate {
            address,
            code: code.to_vector(self.allocator()),
        };
        self.add_action(action);

        Ok(())
    }

    async fn storage(&self, address: Address, index: U256) -> Result<[u8; 32]> {
        self.touched_accounts.touch_storage(address, index);

        let Some(value) = self.actions().find_storage(address, index) else {
            return self.state.storage(address, index).await;
        };

        Ok(*value)
    }

    async fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()> {
        let action = Action::EvmSetStorage {
            address,
            index,
            value,
        };
        self.add_action(action);

        Ok(())
    }

    async fn transient_storage(&self, address: Address, index: U256) -> Result<[u8; 32]> {
        self.state.transient_storage(address, index).await
    }

    fn set_transient_storage(
        &mut self,
        address: Address,
        index: U256,
        value: [u8; 32],
    ) -> Result<()> {
        self.state.set_transient_storage(address, index, value)
    }

    async fn block_hash(
        &mut self,
        number: U256,
        context: &crate::evm::Context,
    ) -> Result<[u8; 32]> {
        self.state.block_hash(number, context).await
    }

    async fn block_number(&mut self, context: &crate::evm::Context) -> Result<U256> {
        self.state.block_number(context).await
    }

    async fn block_timestamp(&mut self, context: &crate::evm::Context) -> Result<U256> {
        self.state.block_timestamp(context).await
    }

    async fn precompile_extension(
        &mut self,
        context: &crate::evm::Context,
        address: &Address,
        data: &[u8],
        is_static: bool,
    ) -> Option<Result<Vec<u8>>> {
        call_precompile_extension(self, context, address, data, is_static).await
    }

    fn snapshot(&mut self) {
        self.state.snapshot();
    }

    fn revert_snapshot(&mut self) {
        self.state.revert_snapshot();
    }

    fn commit_snapshot(&mut self) {
        self.state.commit_snapshot();
    }
}

#[maybe_async(?Send)]
impl<A, P> PrecompileDatabase for ExecutorState<'_, A, P>
where
    A: Allocator + Copy,
    P: Platform,
{
    type AccountRaw = P::AccountRaw;

    fn program_id(&self) -> &Pubkey {
        self.state.program_id()
    }

    fn operator(&self) -> &Pubkey {
        self.state.operator()
    }

    fn chains(&self) -> impl Iterator<Item = Chain> {
        self.state.chains()
    }

    fn keys(&self) -> &impl KeysIndex {
        self.state.keys()
    }

    async fn use_raw_account<R, F>(&self, pubkey: &Pubkey, f: F) -> Result<R>
    where
        F: FnOnce(Self::AccountRaw) -> R,
    {
        self.touched_accounts.touch_solana(pubkey);

        self.state.use_raw_account(pubkey, f).await
    }

    async fn rent(&self) -> Result<Rent> {
        self.state.rent().await
    }

    fn is_iterative_mode(&self) -> bool {
        true
    }

    async fn invoke(&mut self, _instruction: Instruction, _seeds: &[&[&[u8]]]) -> Result<()> {
        unreachable!("Invoke should not be called in iterative mode, use queue_invoke instead");
    }

    async fn queue_invoke(&mut self, instruction: Instruction, seeds: &[&[&[u8]]]) -> Result<()> {
        let allocator = self.allocator();

        let action = Action::ExternalInstruction {
            program_id: instruction.program_id,
            accounts: instruction.accounts.elementwise_copy_into_vector(allocator),
            data: instruction.data.into_vector(allocator),
            seeds: InvokeSeeds::new(seeds, allocator),
        };
        self.add_action(action);

        Ok(())
    }

    async fn external_account(&self, pubkey: &Pubkey) -> Result<OwnedAccountInfo> {
        self.touched_accounts.touch_solana(pubkey);

        let metas = self.actions().collect_external_accounts();
        if !metas.iter().any(|m| (&m.pubkey == pubkey) && m.is_writable) {
            let account = self.state.external_account(pubkey).await?;
            return Ok(account);
        }

        let mut accounts = VectorMap::<Pubkey, OwnedAccountInfo>::new();

        for m in metas {
            self.touched_accounts.touch_solana(&m.pubkey);

            let entry = accounts.entry(m.pubkey);
            if let vector_map::Entry::Vacant(entry) = entry {
                let account = if m.pubkey == FAKE_OPERATOR {
                    OwnedAccountInfo::fake_operator()
                } else {
                    self.state.external_account(&m.pubkey).await?
                };
                entry.insert(account);
            }
        }

        let rent = self.rent().await?;
        self.actions()
            .apply_to_external_accounts(&rent, &mut accounts)?;

        let account = accounts.into_single_value(pubkey).unwrap();
        Ok(account)
    }

    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        None
    }

    async fn solana_user_pubkey(&self, address: Address) -> Result<Option<Pubkey>> {
        let Some(sol_chain) = self.chains().find(|c| c.name == "sol") else {
            return Ok(None);
        };

        self.touched_accounts.touch_balance(address, sol_chain.id);

        let pubkey = self
            .state
            .platform()
            .get_balance(address, sol_chain.id)
            .await?
            .map(|b| b.solana_address())
            .unwrap_or_default();

        Ok(pubkey)
    }

    async fn burn(&mut self, source: Address, chain_id: u64, value: U256) -> Result<()> {
        if value == U256::ZERO {
            return Ok(());
        }

        let source_balance = self.balance_indirect(source, chain_id).await?;
        if source_balance < value {
            return Err(Error::InsufficientBalance(source, chain_id, value));
        }

        let action = Action::Burn {
            source,
            chain_id,
            value,
        };
        self.add_action(action);

        Ok(())
    }
}
