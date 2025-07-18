#![allow(irrefutable_let_patterns)]
use ethnum::U256;
use solana_program::{
    account_info::AccountInfo, instruction::Instruction, log::sol_log_data,
    program::invoke_signed_unchecked, pubkey::Pubkey, rent::Rent, system_instruction,
    system_program, sysvar::Sysvar,
};

use crate::{
    account::{
        Account, AccountDispatch, BalanceAccount, ContainerAccount, Operator, OperatorBalance,
        OperatorBalanceValidator, Root, TransactionTree, Treasury, TAG_CONTAINER,
    },
    config::PAYMENT_TO_TREASURE,
    debug::log_data,
    error::{Error, Result},
    gasometer::Gasometer,
    platform::keys_index::KeysIndex,
    types::{Address, Transaction},
};

use super::{keys_index::CachedKeysIndex, Chain, InvokeMode, Platform, FAKE_OPERATOR};

pub struct Solana<'a> {
    sorted_account_infos: Vec<AccountInfo<'a>>,
    containers: Vec<ContainerAccount<'a>>,
    panic_on_revert: bool,
    pub operator: Operator<'a>,
    pub operator_balance: Option<OperatorBalance<'a>>,
    keys_index: CachedKeysIndex,
    gasometer: Gasometer,
}

impl<'a> Solana<'a> {
    pub fn new(
        accounts: &[AccountInfo<'a>],
        operator: Operator<'a>,
        operator_balance: Option<OperatorBalance<'a>>,
    ) -> Result<Self> {
        let mut sorted_account_infos = accounts.to_vec();
        sorted_account_infos.sort_unstable_by_key(|a| a.key);

        let mut containers = Vec::with_capacity(4);
        for account_info in &sorted_account_infos {
            if !account_info.tag_is(crate::ID, TAG_CONTAINER) {
                continue;
            }

            let account = account_info.clone().into();
            let container = unsafe { ContainerAccount::from_account_unchecked(account) };
            containers.push(container);
        }

        if let Some(balance) = &operator_balance {
            balance.validate_owner(&operator)?;
        }

        let mut gasometer = Gasometer::new(&operator);
        gasometer.record_address_lookup_table(&sorted_account_infos);

        Ok(Self {
            sorted_account_infos,
            containers,
            panic_on_revert: false,
            operator,
            operator_balance,
            keys_index: CachedKeysIndex::new(crate::ID),
            gasometer,
        })
    }

    pub fn new_with_solana_call(
        accounts: &[AccountInfo<'a>],
        operator: Operator<'a>,
        operator_balance: Option<OperatorBalance<'a>>,
    ) -> Result<Self> {
        let mut solana = Self::new(accounts, operator, operator_balance)?;
        solana.panic_on_revert();

        Ok(solana)
    }

    #[inline(always)]
    pub fn panic_on_revert(&mut self) {
        self.panic_on_revert = true;
    }

    #[inline(always)]
    pub fn try_find_account_info(&self, pubkey: Pubkey) -> Option<&AccountInfo<'a>> {
        let Ok(index) = self
            .sorted_account_infos
            .binary_search_by_key(&&pubkey, |a| a.key)
        else {
            return None;
        };

        // We just got an 'index' from the binary_search over this vector.
        Some(unsafe { self.sorted_account_infos.get_unchecked(index) })
    }

    #[track_caller]
    pub fn find_account_info(&self, pubkey: Pubkey) -> &AccountInfo<'a> {
        let Some(info) = self.try_find_account_info(pubkey) else {
            panic_with_error!(Error::AccountMissing(pubkey))
        };

        info
    }

    pub fn update_accounts_lamports(&mut self) -> Result<()> {
        let rent = Rent::get()?;
        let mut expanded_accounts = Vec::with_capacity(self.sorted_account_infos.len());

        for account in &self.sorted_account_infos {
            if !crate::check_id(account.owner) {
                continue;
            }

            let original_data_len = unsafe { account.original_data_len() };
            if original_data_len == account.data_len() {
                continue;
            }

            let minimum_balance = rent.minimum_balance(account.data_len());
            if account.lamports() >= minimum_balance {
                continue;
            }

            let lamports = minimum_balance - account.lamports();
            expanded_accounts.push((account, lamports));
        }

        if expanded_accounts.is_empty() {
            return Ok(());
        }

        // We collect all lamports to a single account and distribute them later
        // This is required avoid multiple calls to `invoke_signed`
        // Because the number of `invoke_signed` in the transaction is limited
        let (collector, _) = expanded_accounts[0];
        let total_lamports = expanded_accounts.iter().fold(0_u64, |total, v| total + v.1);

        let operator: &AccountInfo = &self.operator.info;
        let system: &AccountInfo = self.find_account_info(system_program::ID);
        invoke_signed_unchecked(
            &system_instruction::transfer(operator.key, collector.key, total_lamports),
            &[system.clone(), operator.clone(), collector.clone()],
            &[],
        )?;

        for (account, lamports) in expanded_accounts {
            **collector.lamports.borrow_mut() -= lamports;
            **account.lamports.borrow_mut() += lamports;
        }

        Ok(())
    }

    pub fn use_gasometer<R, F>(&mut self, action: F) -> R
    where
        F: FnOnce(&mut Gasometer) -> R,
    {
        action(&mut self.gasometer)
    }

    pub fn reward_operator_from_holder(&mut self, state: &mut Root) -> Result<()> {
        let chain_id = state.tx_chain_id().unwrap_or_else(|| self.default_chain());

        let gas = self.gasometer.collect_gas(&self.operator);

        state.consume_gas(gas)?;
        let total_gas = state.gas_used();

        log_data(&[b"GAS", &gas.to_le_bytes(), &total_gas.to_le_bytes()]);

        let gas_price = state.gas_price();
        let Some(tokens) = gas.checked_mul(gas_price) else {
            return Err(Error::IntegerOverflow);
        };

        if tokens == U256::ZERO {
            return Ok(());
        }

        let Some(balance) = self.operator_balance.as_mut() else {
            return Err(Error::OperatorBalanceMissing);
        };

        if balance.chain_id() != chain_id {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        balance.mint(tokens)
    }

    pub fn reward_operator_from_tree(
        &mut self,
        tree: &mut TransactionTree,
        transaction_hash: [u8; 32],
    ) -> Result<()> {
        let gas_limit = tree.gas_limit(transaction_hash)?;
        let gas = self.gasometer.collect_gas(&self.operator);

        if gas > gas_limit {
            return Err(Error::OutOfGas(gas_limit, gas));
        }

        log_data(&[b"GAS", &gas.to_le_bytes(), &gas.to_le_bytes()]);

        let gas_price = tree.max_fee_per_gas();
        let Some(tokens) = gas.checked_mul(gas_price) else {
            return Err(Error::IntegerOverflow);
        };

        if tokens == U256::ZERO {
            return Ok(());
        }

        let Some(balance) = self.operator_balance.as_mut() else {
            return Err(Error::OperatorBalanceMissing);
        };

        if balance.chain_id() != tree.chain_id() {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        tree.burn(tokens)?;
        balance.mint(tokens)
    }

    pub fn reward_operator_from_origin(
        &mut self,
        origin: Address,
        transaction: &Transaction,
    ) -> Result<()> {
        let chain_id = transaction.chain_id(self);

        let gas_limit = transaction.gas_limit();
        let gas = self.gasometer.collect_gas(&self.operator);

        if gas > gas_limit {
            return Err(Error::OutOfGas(gas_limit, gas));
        }

        log_data(&[b"GAS", &gas.to_le_bytes(), &gas.to_le_bytes()]);

        let gas_price = transaction.gas_price();
        let Some(tokens) = gas.checked_mul(gas_price) else {
            return Err(Error::IntegerOverflow);
        };

        if tokens == U256::ZERO {
            return Ok(());
        }

        let mut origin_balance: BalanceAccount = self.create_balance(origin, chain_id)?;
        let Some(operator_balance) = self.operator_balance.as_mut() else {
            return Err(Error::OperatorBalanceMissing);
        };

        if operator_balance.chain_id() != chain_id {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        origin_balance.burn(tokens)?;
        operator_balance.mint(tokens)
    }

    pub fn withdraw_operator_balance(&mut self) -> Result<()> {
        let Some(mut operator_balance) = self.operator_balance.clone() else {
            return Err(Error::OperatorBalanceMissing);
        };

        let address = operator_balance.address();
        let chain_id = operator_balance.chain_id();

        let mut target: BalanceAccount = self.create_balance(address, chain_id)?;
        operator_balance.withdraw(&mut target)
    }

    #[inline(always)]
    pub fn log_miner_address(&self, origin: Address) {
        let address = self.operator_balance.miner(origin);
        log_data(&[b"MINER", address.as_bytes()]);
    }

    pub fn pay_to_treasury(&mut self, treasury: Treasury<'a>) -> Result<()> {
        let system = self.find_account_info(system_program::ID);
        let operator = &self.operator.info;

        invoke_signed_unchecked(
            &system_instruction::transfer(operator.key, treasury.key, PAYMENT_TO_TREASURE),
            &[system.clone(), operator.clone(), treasury.into()],
            &[],
        )
        .map_err(Error::from)
    }
}

#[maybe_async::sync_impl]
impl<'a> Platform<'a> for Solana<'a> {
    #[inline(always)]
    fn program_id(&self) -> Pubkey {
        crate::ID
    }

    #[inline(always)]
    fn operator(&self) -> Pubkey {
        *self.operator.key
    }

    #[inline(always)]
    fn chains(&self) -> impl Iterator<Item = Chain> {
        crate::config::CHAIN_ID_LIST.iter().map(|c| Chain {
            id: c.0,
            name: c.1.to_string(),
            token: c.2,
        })
    }

    #[inline(always)]
    fn default_chain(&self) -> u64 {
        crate::config::DEFAULT_CHAIN_ID
    }

    #[inline(always)]
    fn keys(&self) -> &impl KeysIndex {
        &self.keys_index
    }

    fn invoke(
        &mut self,
        mut instruction: Instruction,
        seeds: &[&[&[u8]]],
        _mode: InvokeMode,
    ) -> Result<()> {
        for meta in &mut instruction.accounts {
            if (meta.pubkey == self.operator()) || (meta.pubkey == self.program_id()) {
                return Err(Error::InvalidAccountForCall(meta.pubkey));
            }

            if meta.pubkey == FAKE_OPERATOR {
                meta.pubkey = self.operator();
            }
        }

        invoke_signed_unchecked(&instruction, &self.sorted_account_infos, seeds)
            .map_err(Error::from)
    }

    #[inline(always)]
    fn get_return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        solana_program::program::get_return_data()
    }

    #[rustfmt::skip]
    #[inline(always)]
    fn log_event<const N: usize>(&mut self, address: Address, topics: [[u8; 32]; N], data: &[u8]) {
        let address = address.as_bytes();

        match N {
            0 => sol_log_data(&[b"LOG0", address, &[0], data]),
            1 => sol_log_data(&[b"LOG1", address, &[1], &topics[0], data]),
            2 => sol_log_data(&[b"LOG2", address, &[2], &topics[0], &topics[1], data]),
            3 => sol_log_data(&[b"LOG3", address, &[3], &topics[0], &topics[1], &topics[2], data]),
            4 => sol_log_data(&[b"LOG4", address, &[4], &topics[0], &topics[1], &topics[2], &topics[3], data]),
            _ => unreachable!(),
        }
    }

    #[inline(always)]
    fn get_sysvar<T: Sysvar>(&self) -> Result<T> {
        let sysvar = T::get()?;
        Ok(sysvar)
    }

    #[inline(always)]
    fn get_sysvar_part<T: Sysvar>(&self, offset: usize, buffer: &mut [u8]) -> Result<()> {
        let buffer_addr = buffer.as_mut_ptr();

        let sysvar_id = T::id();
        let sysvar_id_addr: *const u8 = sysvar_id.as_ref().as_ptr();

        let offset: u64 = offset.try_into()?;
        let length: u64 = buffer.len().try_into()?;

        let status = unsafe {
            solana_program::syscalls::sol_get_sysvar(sysvar_id_addr, buffer_addr, offset, length)
        };

        match status {
            solana_program::entrypoint::SUCCESS => Ok(()),
            e => Err(Error::ProgramError(e.into())),
        }
    }

    #[track_caller]
    fn get_account(&self, pubkey: Pubkey) -> Result<Account<'a>> {
        // Ether we have almost all accounts in containers or no containers at all
        for container in &self.containers {
            let Ok(account) = container.account(pubkey) else {
                continue;
            };

            return Ok(account.into());
        }

        self.get_real_account(pubkey)
    }

    #[track_caller]
    fn get_real_account(&self, pubkey: Pubkey) -> Result<Account<'a>> {
        let info = self.find_account_info(pubkey);
        Ok(Account::from(info.clone()))
    }

    fn assign_account(&mut self, seeds: &[&[u8]]) -> Result<Account<'a>> {
        let pubkey = Pubkey::create_program_address(seeds, &crate::ID)?;
        let account: Account<'a> = self.get_account(pubkey)?;

        if account.is_system_owned() {
            let system = self.find_account_info(system_program::ID);
            let account_info = account.as_account_info();

            let assign = system_instruction::assign(&pubkey, &crate::ID);
            let account_infos = &[system.clone(), account_info.clone()];
            invoke_signed_unchecked(&assign, account_infos, &[seeds])?;
        }

        Ok(account)
    }

    fn assign_account_with_seed(
        &mut self,
        base: Pubkey,
        seed: &str,
        base_seeds: &[&[u8]],
    ) -> Result<Account<'a>> {
        let pubkey = Pubkey::create_with_seed(&base, seed, &crate::ID)?;
        let account: Account<'a> = self.get_account(pubkey)?;

        if account.is_system_owned() {
            let system = self.find_account_info(system_program::ID);
            let base_account = self.find_account_info(base);
            let account_info = account.as_account_info();

            let assign = system_instruction::assign_with_seed(&pubkey, &base, seed, &crate::ID);
            let account_infos = &[system.clone(), account_info.clone(), base_account.clone()];
            invoke_signed_unchecked(&assign, account_infos, &[base_seeds])?;
        }

        Ok(account)
    }

    #[inline(always)]
    fn snapshot(&mut self) {
        // not supported on Solana
        // do nothing
    }

    #[inline(always)]
    fn revert(&mut self) {
        if self.panic_on_revert {
            panic_with_error!(Error::RevertWithSolanaCall);
        }
    }

    #[inline(always)]
    fn commit(&mut self) {
        // do nothing
    }
}
