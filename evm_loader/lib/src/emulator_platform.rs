use std::cell::{Cell, RefCell, RefMut};
use std::collections::HashSet;
use std::collections::{hash_map::Entry, HashMap};

use async_trait::async_trait;
use evm_loader::account::{
    AccountDispatch, ContainerAccount, ReferenceAccount, SharedAccount, TAG_CONTAINER,
    TAG_REFERENCE,
};
use evm_loader::error::{Error, Result};
use evm_loader::platform::{DefaultKeysIndex, InvokeMode, KeysIndex, FAKE_OPERATOR};
use evm_loader::{
    account::Account,
    platform::{Chain as ProgramChain, Platform},
    types::Address,
};
use solana_account_decoder::UiDataSliceConfig;
use solana_sdk::account::Account as SolanaSdkAccount;
use solana_sdk::clock::Clock;
use solana_sdk::program_error::ProgramError;
use solana_sdk::rent::Rent;
use solana_sdk::sysvar::SysvarId;
use solana_sdk::transaction_context::TransactionReturnData;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, sysvar::Sysvar};

use crate::account_data::AccountData;
use crate::commands::emulate::SolanaAccount;
use crate::commands::get_config::ChainInfo;
use crate::rpc::{CachedRpc, Rpc};
use crate::solana_simulator::{instruction_error_to_string, SolanaSimulator};
use crate::sysvar::get_sysvar;
use crate::tracing::{AccountOverride, BlockOverrides};
use crate::types::AccountInfoLevel;
use crate::{NeonError, NeonResult};

#[must_use]
const fn fake_operator_account() -> SolanaSdkAccount {
    SolanaSdkAccount {
        lamports: 100 * 1_000_000_000,
        data: vec![],
        owner: solana_sdk_ids::system_program::ID,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

#[derive(Default, Clone, Copy)]
pub struct ExecuteStatus {
    pub external_solana_call: bool,
    pub reverts_before_solana_calls: bool,
    pub reverts_after_solana_calls: bool,
}

pub struct EmulatorPlatform<R: Rpc> {
    pub rpc: CachedRpc<R>,
    keys_index: DefaultKeysIndex,

    program_id: Pubkey,
    chains: Vec<ChainInfo>,

    return_data: Option<TransactionReturnData>,

    stack: Vec<RefCell<HashMap<Pubkey, SharedAccount>>>,
    logs: Vec<Vec<web3::types::Log>>,

    clock_used: Cell<bool>,
    execute_status: ExecuteStatus,
}

impl<R: Rpc> EmulatorPlatform<R> {
    pub async fn new(
        rpc: R,
        program_id: Pubkey,
        chains: &[ChainInfo],
        accounts_hint: &[Pubkey],
    ) -> Result<Self> {
        let rpc = CachedRpc::new(rpc);
        rpc.cache_accounts(accounts_hint).await;
        rpc.insert_to_cache(FAKE_OPERATOR, Some(fake_operator_account()));
        Ok(Self {
            rpc,
            keys_index: DefaultKeysIndex::new(program_id),
            program_id,
            chains: chains.to_vec(),
            return_data: None,
            stack: vec![RefCell::new(HashMap::new())],
            logs: vec![Vec::new()],
            clock_used: Cell::new(false),
            execute_status: ExecuteStatus::default(),
        })
    }

    pub async fn override_clock(&mut self, overrides: BlockOverrides) -> Result<()> {
        let mut clock: Clock = get_sysvar(&self.rpc)
            .await
            .map_err(|e| Error::Custom(e.to_string()))?;

        if let Some(number) = overrides.number {
            clock.slot = number;
        }
        if let Some(time) = overrides.time {
            clock.unix_timestamp = time;
        }

        let account = SolanaSdkAccount {
            lamports: 1,
            data: bincode::serialize(&clock)?,
            owner: solana_sdk::sysvar::id(),
            executable: false,
            rent_epoch: 0,
        };
        self.rpc.insert_to_cache(Clock::id(), Some(account));

        Ok(())
    }

    pub fn override_solana_accounts<I>(&mut self, overrides: I)
    where
        I: IntoIterator<Item = (Pubkey, Option<SolanaSdkAccount>)>,
    {
        for (pubkey, account) in overrides {
            self.rpc.insert_to_cache(pubkey, account);
        }
    }

    pub async fn override_accounts<I>(&mut self, chain_id: u64, overrides: I) -> Result<()>
    where
        I: IntoIterator<Item = (Address, AccountOverride)>,
    {
        for (address, account_override) in overrides {
            if let Some(nonce) = account_override.nonce {
                let mut account = self.create_balance(address, chain_id).await?;
                account.override_nonce_by(nonce);
            }

            if let Some(balance) = account_override.balance {
                let mut account = self.create_balance(address, chain_id).await?;
                account.override_balance_by(balance);
            }

            if let Some(web3::types::Bytes(code)) = account_override.code {
                let mut account = self.create_contract(address, chain_id).await?;
                account.allocate_entire_code_buffer(&code)?;
                account.set_code(&code)?;
            }

            // TODO: storage cells overrides
        }

        Ok(())
    }

    pub fn is_clock_used(&self) -> bool {
        self.clock_used.get()
    }

    pub const fn execute_status(&self) -> ExecuteStatus {
        self.execute_status
    }

    pub fn logs(&self) -> &[web3::types::Log] {
        self.logs.last().unwrap()
    }

    pub fn used_accounts(&self) -> Vec<SharedAccount> {
        let stack = self.current_stack_frame();
        stack
            .values()
            .filter(|a| a.pubkey() != FAKE_OPERATOR)
            .cloned()
            .collect()
    }

    pub fn used_solana_accounts(&self) -> Vec<SolanaAccount> {
        self.used_accounts()
            .iter()
            .map(|v| SolanaAccount {
                pubkey: v.pubkey(),
                is_writable: v.is_modified(),
            })
            .collect()
    }

    pub fn provide_account_data(&self, level: AccountInfoLevel) -> NeonResult<Vec<AccountData>> {
        let mut result = Vec::new();

        for account in &self.used_accounts() {
            if level == AccountInfoLevel::Changed && !account.is_modified() {
                continue;
            }

            let sdk_account: solana_sdk::account::Account = account.into();
            let account_data = AccountData::new_from_account(account.pubkey(), &sdk_account);
            result.push(account_data);
        }

        Ok(result)
    }

    pub async fn required_lamports(&self) -> Result<u64> {
        let stack = self.current_stack_frame();
        let mut total_lamports = 0_u64;

        // Calculate accounts expansion cost
        let rent: Rent = self.get_sysvar().await?;
        for account in stack.values() {
            if account.owner() != self.program_id {
                continue;
            }

            let data_len = account.data_len();
            let minimum_balance = rent.minimum_balance(data_len);
            let required_lamports = minimum_balance.saturating_sub(account.lamports());

            total_lamports = total_lamports.saturating_add(required_lamports);
        }

        // Calculate other operator spending
        if let Some(operator) = stack.get(&FAKE_OPERATOR) {
            let fake = fake_operator_account();

            if fake.lamports > operator.lamports() {
                let spend_lamports = fake.lamports - operator.lamports();
                total_lamports = total_lamports.saturating_add(spend_lamports);
            } else {
                let aquired_lamports = operator.lamports() - fake.lamports;
                total_lamports = total_lamports.saturating_sub(aquired_lamports);
            }
        }

        Ok(total_lamports)
    }

    pub fn realloc_iterations(&self) -> u64 {
        let mut max_data_increase = 0_usize;

        for account in self.used_accounts() {
            if account.owner() != self.program_id {
                continue;
            }

            let data_len = account.data_len();
            let original_data_len = account.original_data_len();
            let data_increase = data_len.saturating_sub(original_data_len);

            if data_increase > max_data_increase {
                max_data_increase = data_increase;
            }
        }

        (max_data_increase / solana_sdk::entrypoint::MAX_PERMITTED_DATA_INCREASE) as u64
    }

    pub async fn account_from_stack(&self, pubkey: Pubkey) -> Result<SharedAccount> {
        let mut stack_frame = self.current_stack_frame();

        // Search for the account in the current stack frame
        let account = match stack_frame.entry(pubkey) {
            Entry::Occupied(entry) => {
                // Exists in the current stack frame, just return it
                entry.get().clone()
            }
            Entry::Vacant(entry) => {
                // Not found in the current stack frame, fetch it from the RPC
                let account = match self.rpc.get_account(&pubkey).await {
                    Ok(Some(account)) => SharedAccount::new(pubkey, &account),
                    Ok(None) => SharedAccount::new_empty(pubkey),
                    Err(client_error) => {
                        let emulator_error = NeonError::ClientError(client_error);
                        return Err(Error::Custom(emulator_error.to_string()));
                    }
                };

                entry.insert(account).clone()
            }
        };

        Ok(account)
    }

    pub async fn add_accounts_to_stack(&mut self, pubkeys: &[Pubkey]) -> Result<()> {
        let accounts = self.rpc.get_multiple_accounts(pubkeys).await.map_err(|e| {
            let emulator_error = NeonError::ClientError(e);
            Error::Custom(emulator_error.to_string())
        })?;

        let mut stack = self.current_stack_frame();
        for (key, account) in pubkeys.iter().copied().zip(accounts.into_iter()) {
            stack.entry(key).or_insert_with(|| {
                account.map_or_else(
                    || SharedAccount::new_empty(key),
                    |account| SharedAccount::new(key, &account),
                )
            });
        }

        Ok(())
    }

    pub fn current_stack_frame(&self) -> RefMut<HashMap<Pubkey, SharedAccount>> {
        self.stack.last().unwrap().borrow_mut()
    }

    fn update_current_stack_frame(&mut self, data: HashMap<Pubkey, SharedAccount>) {
        let current_stack_frame = self.stack.last_mut().unwrap().get_mut();
        for (pubkey, account) in data {
            current_stack_frame.insert(pubkey, account);
        }
    }

    fn clone_current_stack_frame(&self) -> HashMap<Pubkey, SharedAccount> {
        let current = self.current_stack_frame();
        let mut clone = HashMap::with_capacity(current.len());
        for (pubkey, account) in current.iter() {
            clone.insert(*pubkey, account.deep_clone());
        }

        clone
    }

    fn pop_stack_frame(&mut self) -> HashMap<Pubkey, SharedAccount> {
        self.stack.pop().unwrap().into_inner()
    }

    fn push_stack_frame(&mut self, stack: HashMap<Pubkey, SharedAccount>) {
        self.stack.push(RefCell::new(stack));
    }
}

#[async_trait(?Send)]
impl<'a, R: Rpc> Platform<'a> for EmulatorPlatform<R> {
    fn program_id(&self) -> Pubkey {
        self.program_id
    }

    fn operator(&self) -> Pubkey {
        FAKE_OPERATOR
    }

    fn chains(&self) -> impl Iterator<Item = ProgramChain> {
        self.chains.iter().map(|c| ProgramChain {
            id: c.id,
            name: c.name.clone(),
            token: c.token,
        })
    }

    fn default_chain(&self) -> u64 {
        let neon_chain = self.chains.iter().find(|c| c.name == "neon").unwrap();
        neon_chain.id
    }

    fn keys(&self) -> &impl KeysIndex {
        &self.keys_index
    }

    async fn invoke(
        &mut self,
        instruction: Instruction,
        seeds: &[&[&[u8]]],
        mode: InvokeMode,
    ) -> Result<()> {
        let target_program_id = instruction.program_id;

        if mode == InvokeMode::Normal {
            self.execute_status.external_solana_call = true;
        }

        // Start simulator setup
        let mut simulator = SolanaSimulator::new(self)
            .await
            .map_err(|e| Error::Custom(e.to_string()))?;

        // Prepare accounts
        let signers: HashSet<Pubkey> = seeds
            .iter()
            .map(|seed| Pubkey::create_program_address(seed, &self.program_id).unwrap())
            .collect();

        let mut accounts = Vec::with_capacity(instruction.accounts.len() + 1);
        accounts.push(target_program_id);
        for meta in &instruction.accounts {
            if meta.pubkey != FAKE_OPERATOR && meta.is_signer && !signers.contains(&meta.pubkey) {
                return Err(ProgramError::MissingRequiredSignature.into());
            }
            accounts.push(meta.pubkey);
        }

        // Add accounts to the current stack frame
        self.add_accounts_to_stack(&accounts).await?;

        // Sync accounts with the simulator
        simulator
            .sync_accounts(self, &accounts)
            .await
            .map_err(|e| Error::Custom(e.to_string()))?;

        // Execute the instruction
        let (simulation_result, _) = simulator
            .process_instruction(&instruction)
            .map_err(|e| Error::Custom(e.to_string()))?;

        self.return_data = Some(TransactionReturnData {
            program_id: target_program_id,
            data: simulation_result.return_data,
        });

        if let Err(error) = simulation_result.raw_result {
            let message = instruction_error_to_string(target_program_id, error);
            return Err(Error::ExternalCallFailed(target_program_id, message));
        }

        // Update modified accounts
        let mut stack = self.current_stack_frame();
        for meta in instruction.accounts.iter().filter(|m| m.is_writable) {
            let account_data = simulator.get_account(&meta.pubkey);

            let shared_account = stack.get_mut(&meta.pubkey).unwrap();
            shared_account.update(&account_data);
            shared_account.mark_modified();
        }

        Ok(())
    }

    fn get_return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        self.return_data.clone().map(|r| (r.program_id, r.data))
    }

    async fn log_event<const N: usize>(
        &mut self,
        address: Address,
        topics: [[u8; 32]; N],
        data: &[u8],
    ) {
        let logs = self.logs.last_mut().unwrap();
        logs.push(web3::types::Log {
            address: address.as_bytes().into(),
            topics: topics.iter().map(Into::into).collect(),
            data: data.into(),
            block_hash: None,
            block_number: None,
            transaction_hash: None,
            transaction_index: None,
            log_index: None,
            transaction_log_index: None,
            log_type: None,
            removed: None,
        });
    }

    async fn get_sysvar<T: Sysvar>(&self) -> Result<T> {
        if T::id() == solana_sdk::sysvar::clock::id() {
            self.clock_used.set(true);
        }

        match self.rpc.get_account(&T::id()).await {
            Ok(Some(account)) => {
                let sysvar = bincode::deserialize::<T>(&account.data)?;
                Ok(sysvar)
            }
            Ok(None) => {
                let emulator_error = NeonError::RpcReturnedEmptyAccount(T::id());
                Err(Error::Custom(emulator_error.to_string()))
            }
            Err(client_error) => {
                let emulator_error = NeonError::ClientError(client_error);
                Err(Error::Custom(emulator_error.to_string()))
            }
        }
    }

    async fn get_sysvar_part<T: Sysvar>(&self, offset: usize, buffer: &mut [u8]) -> Result<()> {
        if T::id() == solana_sdk::sysvar::clock::id() {
            self.clock_used.set(true);
        }

        let slice_config = UiDataSliceConfig {
            offset,
            length: buffer.len(),
        };

        match self
            .rpc
            .get_account_slice(&T::id(), Some(slice_config))
            .await
        {
            Ok(Some(account)) => {
                buffer.copy_from_slice(&account.data);
                Ok(())
            }
            Ok(None) => {
                let emulator_error = NeonError::RpcReturnedEmptyAccount(T::id());
                Err(Error::Custom(emulator_error.to_string()))
            }
            Err(client_error) => {
                let emulator_error = NeonError::ClientError(client_error);
                Err(Error::Custom(emulator_error.to_string()))
            }
        }
    }

    async fn get_account(&self, pubkey: Pubkey) -> Result<Account<'a>> {
        let account = self.rpc.get_account(&pubkey).await.map_err(|e| {
            let emulator_error = NeonError::ClientError(e);
            Error::Custom(emulator_error.to_string())
        })?;

        let Some(account) = account else {
            return self.get_real_account(pubkey).await;
        };

        let account = SharedAccount::new(pubkey, &account);

        if account.tag_is(self.program_id, TAG_CONTAINER) {
            let container_account = self.account_from_stack(pubkey).await?.into();
            let container = ContainerAccount::from_account(self.program_id, container_account)?;

            let account_in_container = container.account(pubkey)?;
            return Ok(account_in_container.into());
        }

        if account.tag_is(self.program_id, TAG_REFERENCE) {
            let reference_account = account.into();
            let reference = ReferenceAccount::from_account(self.program_id, reference_account)?;

            let container_account = self.account_from_stack(reference.container()).await?.into();
            let container = ContainerAccount::from_account(self.program_id, container_account)?;

            let account_in_container = container.account(pubkey)?;
            return Ok(account_in_container.into());
        }

        self.get_real_account(pubkey).await
    }

    async fn get_real_account(&self, pubkey: Pubkey) -> Result<Account<'a>> {
        let account = self.account_from_stack(pubkey).await?;
        Ok(account.into())
    }

    async fn assign_account(&mut self, seeds: &[&[u8]]) -> Result<Account<'a>> {
        let pubkey = Pubkey::create_program_address(seeds, &self.program_id)?;
        let account = Platform::get_account(self, pubkey).await?;

        if account.is_system_owned() {
            let account = account.as_shared_account();

            account.assign(self.program_id);
            account.mark_modified();
        }

        Ok(account)
    }

    async fn assign_account_with_seed(
        &mut self,
        base: Pubkey,
        seed: &str,
        base_seeds: &[&[u8]],
    ) -> Result<Account<'a>> {
        let calculated_base = Pubkey::create_program_address(base_seeds, &self.program_id)?;
        assert_eq!(calculated_base, base); // `base` is passed as a parameter to avoid calculation on the program side. Verify it here.

        let pubkey = Pubkey::create_with_seed(&base, seed, &self.program_id)?;
        let account = Platform::get_account(self, pubkey).await?;

        if account.is_system_owned() {
            let account = account.as_shared_account();

            account.assign(self.program_id);
            account.mark_modified();
        }

        Ok(account)
    }

    fn snapshot(&mut self) {
        self.logs.push(Vec::new());

        let new_stack_frame = self.clone_current_stack_frame();
        self.push_stack_frame(new_stack_frame);
    }

    fn revert(&mut self) {
        if self.execute_status.external_solana_call {
            self.execute_status.reverts_after_solana_calls = true;
        } else {
            self.execute_status.reverts_before_solana_calls = true;
        }

        // discard from the last stack frame
        let _ = self.logs.pop();

        // Revert accounts to the previous stack frame
        let stack_frame = self.pop_stack_frame();
        stack_frame.values().for_each(SharedAccount::revert);
        self.update_current_stack_frame(stack_frame);
    }

    fn commit(&mut self) {
        let mut logs = self.logs.pop().unwrap();
        self.logs.last_mut().unwrap().append(&mut logs);

        // Add current accounts to the previous stack frame
        let stack_frame = self.pop_stack_frame();
        self.update_current_stack_frame(stack_frame);
    }
}
