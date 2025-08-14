use allocator_api2::alloc::Allocator;
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::clock::Clock;
use solana_program::instruction::{AccountMeta, Instruction};
use solana_program::pubkey::Pubkey;
use solana_sdk_ids::{bpf_loader, system_program};

use crate::account::{Account, AccountDispatch};
use crate::allocator::StateAllocator;
use crate::config::{GAS_LIMIT_MULTIPLIER_NO_CHAINID, NO_UPDATE_TRACKING_OWNERS};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::{ExitStatus, Machine, SolanaCallInterrupt};
use crate::executor::{ExecutorState, ExecutorStateData, TouchedAccounts};
use crate::platform::Platform;
use crate::types::seeds::{Seeds, SeedsRef};
use crate::types::vector::{vector_map::Entry, VectorMap, VectorSliceExt, VectorSliceSlowExt};
use crate::types::{Address, Transaction, TransactionType, Vector};

use super::{
    BalanceAccount, ContractAccount, StorageCell, TransactionTree, TAG_ACCOUNT_BALANCE,
    TAG_ACCOUNT_CONTRACT, TAG_STORAGE_CELL,
};

#[derive(PartialEq, Eq)]
pub enum AccountsStatus {
    Ok,
    NeedRestart,
}

#[derive(Clone, PartialEq, Eq, Copy)]
#[repr(C)]
pub enum AccountRevision {
    Revision(u32),
    Hash(solana_program::hash::Hash),
}

impl Default for AccountRevision {
    fn default() -> Self {
        AccountRevision::Revision(0)
    }
}

impl AccountRevision {
    pub fn new(program_id: Pubkey, info: Account) -> Self {
        if [bpf_loader::ID, system_program::ID].contains(&info.owner()) {
            return AccountRevision::Revision(0);
        }

        if NO_UPDATE_TRACKING_OWNERS.contains(&info.owner()) {
            let hash = solana_program::hash::Hash::default();
            return AccountRevision::Hash(hash);
        }

        if info.owner() != program_id {
            let hash = solana_program::hash::hashv(&[
                info.owner().as_ref(),
                &info.lamports().to_le_bytes(),
                &info.data(),
            ]);

            return AccountRevision::Hash(hash);
        }

        match info.tag(program_id) {
            Ok(TAG_STORAGE_CELL) => {
                let cell = unsafe { StorageCell::from_account_unchecked(info) };
                Self::Revision(cell.revision())
            }
            Ok(TAG_ACCOUNT_CONTRACT) => {
                let contract = unsafe { ContractAccount::from_account_unchecked(info) };
                Self::Revision(contract.revision())
            }
            Ok(TAG_ACCOUNT_BALANCE) => {
                let balance = unsafe { BalanceAccount::from_account_unchecked(info) };
                Self::Revision(balance.revision())
            }
            _ => Self::Revision(0),
        }
    }
}

#[repr(C)]
pub struct InterruptedInstruction<A: Allocator> {
    pub program_id: Pubkey,
    pub accounts: Vector<AccountMeta, A>,
    pub data: Vector<u8, A>,
}

#[repr(C)]
pub struct InterruptedState<A: Allocator> {
    instruction: InterruptedInstruction<A>,
    signer_seeds: Seeds,
    lamports: Option<u64>,
}

impl<A: Allocator + Copy> InterruptedState<A> {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        instruction: Instruction,
        signer_seeds: &[&[u8]],
        lamports: Option<u64>,
        allocator: A,
    ) -> Self {
        Self {
            instruction: InterruptedInstruction {
                program_id: instruction.program_id,
                accounts: instruction.accounts.elementwise_copy_to_vector(allocator),
                data: instruction.data.to_vector(allocator),
            },
            signer_seeds: Seeds::new(signer_seeds),
            lamports,
        }
    }
}

impl<A: Allocator> InterruptedState<A> {
    pub fn instruction(&self) -> Instruction {
        Instruction {
            program_id: self.instruction.program_id,
            accounts: self.instruction.accounts.to_vec(),
            data: self.instruction.data.to_vec(),
        }
    }

    pub fn seeds(&self) -> SeedsRef {
        SeedsRef::new(&self.signer_seeds)
    }

    pub fn lamports(&self) -> Option<u64> {
        self.lamports
    }
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct PlainData {
    // We may want to extend the PlainData, so the first field
    // indicates that some trailing fields could be filled with garbage
    // and shouldn't be reused
    pub layout_version: usize, // = 0
    /// Ethereum transaction caller address
    pub origin: Address,
    /// Address of the tree account (present for scheduled transactions).
    pub tree_account: Option<Pubkey>,

    pub tx_chain_id: Option<u64>,

    pub gas_used: U256,
    pub gas_limit: U256,
    pub gas_price: U256,

    /// Steps executed in the transaction
    pub steps_executed: u64,

    /// Store the exit status here specifically for scheduled transactions.
    /// Entire `Root` object may not be available in the time of the scheduled finalization,
    pub scheduled_exit_status: Option<(
        crate::account::transaction_tree::Status,
        solana_program::keccak::Hash,
    )>,
    // fields for layout_version >= 1
    // WARNING: If you add new fields here, update `Root::new_after_reset` to read only `layout_version == 0` fields
}

impl PlainData {
    pub fn layout_version() -> usize {
        0
    }

    pub fn new(tx: &dyn Transaction, origin: Address, tree: Option<&TransactionTree>) -> Self {
        assert!(
            !(tx.is(TransactionType::Scheduled) ^ tree.is_some()),
            "Tree account should be present iff it's a scheduled transaction."
        );

        Self {
            layout_version: Self::layout_version(),
            origin,
            tree_account: tree.map(TransactionTree::pubkey),
            tx_chain_id: tx.chain_id(),
            gas_used: U256::ZERO,
            gas_limit: tx.gas_limit(),
            gas_price: tx.gas_price(),
            steps_executed: 0_u64,
            scheduled_exit_status: None,
        }
    }
}

#[repr(C)]
pub struct Root<A: Allocator + Copy = StateAllocator> {
    pub plain_data: PlainData,

    /// Stored revision
    pub revisions: VectorMap<Pubkey, AccountRevision, A>,

    /// Accounts that been read during the transaction    
    pub touched_accounts: VectorMap<Pubkey, u64, A>,

    /// State of `execute_external_instruction` at the Solana call interruption breakpoint
    /// None if no Solana call interruption occurs
    pub interrupted_state: Option<InterruptedState<A>>,

    /// Abbreviated `exit_status` for scheduled transactions is stored in the `plain_data` field,
    /// but we also need a full revert message and status code to print into the logs in last iteration
    pub revert_message: Option<Vector<u8, A>>,
    pub exit_status_code: u8,

    pub executor_state: ExecutorStateData<A>,
    pub machine_state: Machine<A>,

    pub allocator: A,
}

// to be sure that solana and x86 size/alignment match
const _: () = {
    assert!(std::mem::align_of::<Root>() == 0x8);
    assert!(std::mem::size_of::<Root>() == 0x5A0);
    assert!(std::mem::offset_of!(Root, revisions) == 0xE0);
};

#[maybe_async]
impl<A: Allocator + Copy> Root<A> {
    pub async fn new<'a>(
        transaction: &dyn Transaction,
        origin: Address,
        tree: Option<&mut TransactionTree<'a>>,
        solana: &mut (impl Platform<'a> + 'a),
        allocator: A,
    ) -> Result<Self> {
        let plain_data = PlainData::new(transaction, origin, tree.as_deref());
        let root = Self::new_with_plain_data(plain_data, transaction, solana, allocator).await?;

        // Burn the gas from the transaction origin or from the tree
        // The burned gas is stored as a `gas_limit` and `gas_used` in the `plain_data`
        if let Some(tree) = tree {
            tree.burn_gas(root.gas_limit(), root.gas_price())?;
        } else {
            let mut origin: BalanceAccount = solana.get_origin(&root).await?;
            origin.burn_gas(root.gas_limit(), root.gas_price())?;
        }

        Ok(root)
    }

    /// # Safety
    ///  Everything related to the state reset is extremely unsafe. Be careful
    pub async unsafe fn new_after_reset<'a>(
        &self,
        transaction: &dyn Transaction,
        solana: &mut (impl Platform<'a> + 'a),
        allocator: A,
    ) -> Result<Self> {
        // SAFETY WARNING: Old heap could no longer exists.
        // We can only read the `plain_data` here. All other fields are invalid.

        // Copy the plain_data from the existing root
        let mut plain_data = self.plain_data;
        plain_data.steps_executed = 0;
        plain_data.scheduled_exit_status = None;

        Self::new_with_plain_data(plain_data, transaction, solana, allocator).await
    }

    async fn new_with_plain_data<'a>(
        plain_data: PlainData,
        transaction: &dyn Transaction,
        solana: &mut (impl Platform<'a> + 'a),
        allocator: A,
    ) -> Result<Self> {
        let (executor_state, machine_state, touched_accounts_during_init) =
            Self::construct_machine(transaction, plain_data.origin, solana, allocator).await?;

        let mut root = Self {
            plain_data,
            revisions: VectorMap::new_in(allocator),
            touched_accounts: VectorMap::new_in(allocator),
            interrupted_state: None,
            executor_state,
            machine_state,
            allocator,
            revert_message: None,
            exit_status_code: 0,
        };
        root.update_touched_accounts(touched_accounts_during_init, solana)
            .await?;

        Ok(root)
    }

    async fn construct_machine<'a>(
        trx: &dyn Transaction,
        origin: Address,
        solana: &mut (impl Platform<'a> + 'a),
        allocator: A,
    ) -> Result<(ExecutorStateData<A>, Machine<A>, TouchedAccounts)> {
        let clock: Clock = solana.get_sysvar().await?;
        let mut executor_state = ExecutorStateData::with_block_params_in(&clock, allocator);

        let (machine, touched_accounts_during_init) = {
            let mut backend = ExecutorState::new_in(solana, &mut executor_state, allocator);
            let machine = Machine::new_in(trx, origin, &mut backend, allocator).await?;
            (machine, backend.into_touched_accounts())
        };

        Ok((executor_state, machine, touched_accounts_during_init))
    }

    pub fn refund_unused_gas_to_tree(&mut self, tree: &mut TransactionTree) -> Result<()> {
        assert_eq!(self.plain_data.tree_account, Some(tree.pubkey()));

        let unused_gas = self.consume_all_unused_gas()?;
        tree.refund_gas(unused_gas, self.gas_price())
    }

    pub async fn refund_unused_gas_to_origin<'a>(
        &mut self,
        solana: &mut impl Platform<'a>,
    ) -> Result<()> {
        assert!(!self.is_scheduled_transaction());

        let unused_gas = self.consume_all_unused_gas()?;
        if unused_gas == U256::ZERO {
            return Ok(());
        }

        let mut origin = solana.get_origin(&*self).await?;
        origin.refund_gas(unused_gas, self.gas_price())
    }

    fn consume_all_unused_gas(&mut self) -> Result<U256> {
        let avaialble = self.gas_available();
        self.consume_gas(avaialble)?;

        Ok(avaialble)
    }

    pub async fn increase_gas_limit_for_transactions_without_chain_id<'a>(
        &mut self,
        platform: &mut impl Platform<'a>,
    ) -> Result<()> {
        assert!(self.tx_chain_id().is_none());

        let real_gas_limit = self.plain_data.gas_limit;

        let multiplier = U256::from(GAS_LIMIT_MULTIPLIER_NO_CHAINID);
        let increased_gas_limit = real_gas_limit.saturating_mul(multiplier);

        // Burn additional gas from the Origin
        let gas_diff = increased_gas_limit - real_gas_limit;
        let mut origin = platform.get_origin(&*self).await?;
        origin.burn_gas(gas_diff, self.gas_price())?;

        self.plain_data.gas_limit = increased_gas_limit;

        Ok(())
    }

    pub fn origin(&self) -> Address {
        self.plain_data.origin
    }
    pub fn tx_chain_id(&self) -> Option<u64> {
        self.plain_data.tx_chain_id
    }

    #[must_use]
    pub fn gas_used(&self) -> U256 {
        self.plain_data.gas_used
    }

    #[must_use]
    pub fn gas_available(&self) -> U256 {
        self.plain_data.gas_limit.saturating_sub(self.gas_used())
    }

    #[must_use]
    pub fn gas_limit(&self) -> U256 {
        self.plain_data.gas_limit
    }

    #[must_use]
    pub fn gas_price(&self) -> U256 {
        self.plain_data.gas_price
    }

    pub fn consume_gas(&mut self, amount: U256) -> Result<()> {
        if amount == U256::ZERO {
            return Ok(());
        }

        let total_gas_used = self.gas_used().saturating_add(amount);
        if total_gas_used > self.gas_limit() {
            return Err(Error::OutOfGas(self.gas_limit(), total_gas_used));
        }

        self.plain_data.gas_used = total_gas_used;

        Ok(())
    }

    pub fn executor(&mut self) -> (&mut ExecutorStateData<A>, &mut Machine<A>) {
        (&mut self.executor_state, &mut self.machine_state)
    }

    pub fn set_exit_status(&mut self, status: &ExitStatus) {
        assert!(self.plain_data.scheduled_exit_status.is_none());
        assert!(self.revert_message.is_none());
        assert!(status.is_execution_finished());

        self.exit_status_code = status.code();
        if let ExitStatus::Revert(message) = status {
            self.revert_message = Some(message.to_vector(self.allocator));
        }

        let scheduled_status = TransactionTree::prepare_exit_status(status);
        self.plain_data.scheduled_exit_status = Some(scheduled_status);
    }

    pub fn is_execution_finished(&self) -> bool {
        self.plain_data.scheduled_exit_status.is_some()
    }

    pub fn have_uncommitted_state(&self) -> bool {
        let data = &self.executor_state;

        let actions = &data.actions;
        let timestamped_contracts = &data.timestamped_contracts;

        !actions.is_empty() || !timestamped_contracts.is_empty()
    }

    pub fn can_be_finalized(&self) -> bool {
        self.is_execution_finished() && !self.have_uncommitted_state()
    }

    pub fn is_execution_interrupted(&self) -> bool {
        self.interrupted_state.is_some()
    }

    pub fn is_scheduled_transaction(&self) -> bool {
        self.plain_data.tree_account.is_some()
    }

    async fn validate_revisions<'a>(&self, platform: &impl Platform<'a>) -> Result<AccountsStatus> {
        let touched_accounts = self
            .touched_accounts
            .iter()
            .filter_map(|(key, counter)| if counter >= &2 { Some(key) } else { None })
            .copied();

        for pubkey in touched_accounts {
            let account: Account<'a> = platform.get_account(pubkey).await?;

            let account_revision = AccountRevision::new(platform.program_id(), account);
            let stored_revision = &self.revisions[&pubkey];

            if stored_revision != &account_revision {
                log_data(&[b"INVALID_REVISION", pubkey.as_ref()]);
                return Ok(AccountsStatus::NeedRestart);
            }
        }

        Ok(AccountsStatus::Ok)
    }

    async fn validate_timestamps<'a>(
        &self,
        platform: &impl Platform<'a>,
    ) -> Result<AccountsStatus> {
        let Some(block) = self.executor_state.inhereted_block_params else {
            return Ok(AccountsStatus::Ok);
        };
        let block_number: u64 = block.number.try_into()?;

        let timestamped_contracts = &self.executor_state.timestamped_contracts;
        for address in timestamped_contracts {
            let Some(contract) = platform.get_contract(*address).await? else {
                continue;
            };

            if contract.timestamp_used_at() > block_number {
                log_data(&[b"INVALID_REVISION", contract.pubkey().as_ref()]);
                return Ok(AccountsStatus::NeedRestart);
            }
        }

        Ok(AccountsStatus::Ok)
    }

    pub async fn validate_accounts<'a>(
        &self,
        platform: &impl Platform<'a>,
    ) -> Result<AccountsStatus> {
        let status = self.validate_revisions(platform).await?;
        if status != AccountsStatus::Ok {
            return Ok(status);
        }

        self.validate_timestamps(platform).await
    }

    pub async fn update_touched_accounts<'a>(
        &mut self,
        touched_accounts: TouchedAccounts,
        solana: &impl Platform<'a>,
    ) -> Result<()> {
        let program_id = solana.program_id();

        for (key, value) in touched_accounts.into_iter(solana.keys()) {
            let touch_counter = self.touched_accounts.entry(key).or_insert(0);
            *touch_counter += value;

            match self.revisions.entry(key) {
                Entry::Occupied(_) => {}
                Entry::Vacant(entry) => {
                    let account: Account<'a> = solana.get_account(key).await?;
                    let revision = AccountRevision::new(program_id, account);
                    entry.insert(revision);
                }
            }
        }

        Ok(())
    }

    pub fn increment_steps_executed(&mut self, steps: u64) -> Result<()> {
        let total_steps = &mut self.plain_data.steps_executed;
        *total_steps = total_steps
            .checked_add(steps)
            .ok_or(Error::IntegerOverflow)?;

        log_data(&[b"STEPS", &steps.to_le_bytes(), &total_steps.to_le_bytes()]);

        Ok(())
    }

    #[allow(clippy::boxed_local)]
    pub fn set_interupted_state(&mut self, interrupt: SolanaCallInterrupt) {
        let instruction: Instruction = interrupt.0;
        let seeds: Vec<&[u8]> = interrupt.1.iter().map(Vec::as_slice).collect();
        let lamports: Option<u64> = interrupt.2;

        let state = InterruptedState::new(instruction, &seeds, lamports, self.allocator);
        self.interrupted_state = Some(state);
    }
}
