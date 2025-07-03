#![allow(irrefutable_let_patterns)]
use std::collections::HashMap;

use solana_program::{
    account_info::AccountInfo, instruction::Instruction, log::sol_log_data,
    program::invoke_signed_unchecked, pubkey::Pubkey, rent::Rent, system_instruction,
    system_program, sysvar::Sysvar,
};

use crate::{
    account::{
        Account, AccountDispatch, BalanceAccount, Operator, OperatorBalance,
        OperatorBalanceValidator, Treasury,
    },
    config::PAYMENT_TO_TREASURE,
    debug::log_data,
    error::{Error, Result},
    types::Address,
};

use super::{Chain, InvokeMode, Platform, FAKE_OPERATOR};

pub struct Solana<'a> {
    sorted_account_infos: Vec<AccountInfo<'a>>,
    containers_index: HashMap<Pubkey, (Pubkey, usize)>,
    panic_on_revert: bool,
    pub operator: Operator<'a>,
    pub operator_balance: Option<OperatorBalance<'a>>,
}

impl<'a> Solana<'a> {
    pub fn new(
        accounts: &[AccountInfo<'a>],
        operator: Operator<'a>,
        operator_balance: Option<OperatorBalance<'a>>,
    ) -> Result<Self> {
        let mut sorted_account_infos = accounts.to_vec();
        sorted_account_infos.sort_unstable_by_key(|a| a.key);
        sorted_account_infos.dedup_by_key(|a| a.key);

        if let Some(balance) = &operator_balance {
            balance.validate_owner(&operator)?;
        }

        Ok(Self {
            sorted_account_infos,
            containers_index: HashMap::new(), // TODO
            panic_on_revert: false,
            operator,
            operator_balance,
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

    pub fn panic_on_revert(&mut self) {
        self.panic_on_revert = true;
    }

    pub fn try_find_account_info(&self, pubkey: Pubkey) -> Option<&AccountInfo<'a>> {
        let Ok(index) = self
            .sorted_account_infos
            .binary_search_by_key(&pubkey, |a| *a.key)
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

    // Explicitly sync all accounts lamports
    pub fn sync_lamports(self) {
        // see `impl Drop for Solana<'_>`
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

impl Drop for Solana<'_> {
    fn drop(&mut self) {
        let rent = Rent::get().unwrap();
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
            return;
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
        )
        .unwrap();

        for (account, lamports) in expanded_accounts {
            **collector.lamports.borrow_mut() -= lamports;
            **account.lamports.borrow_mut() += lamports;
        }
    }
}

#[maybe_async::sync_impl]
impl<'a> Platform<'a> for Solana<'a> {
    fn program_id(&self) -> Pubkey {
        crate::ID
    }

    fn operator(&self) -> Pubkey {
        *self.operator.key
    }

    fn chains(&self) -> impl Iterator<Item = Chain> {
        crate::config::CHAIN_ID_LIST.iter().map(|c| Chain {
            id: c.0,
            name: c.1.to_string(),
            token: c.2,
        })
    }

    fn default_chain(&self) -> u64 {
        crate::config::DEFAULT_CHAIN_ID
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

    fn get_return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        solana_program::program::get_return_data()
    }

    #[rustfmt::skip]
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

    fn get_sysvar<T: Sysvar>(&self) -> Result<T> {
        let sysvar = T::get()?;
        Ok(sysvar)
    }

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
        let Some((container_pubkey, _index)) = self.containers_index.get(&pubkey).copied() else {
            return self.get_real_account(pubkey);
        };

        let _container_info = self.find_account_info(container_pubkey).clone();
        todo!() // containers
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
            let Account::AccountInfo(account) = &account else {
                unreachable!()
            };

            let assign = system_instruction::assign(&pubkey, &crate::ID);
            let account_infos = &[system.clone(), account.clone()];
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
            let Account::AccountInfo(account) = &account else {
                unreachable!()
            };

            let assign = system_instruction::assign_with_seed(&pubkey, &base, seed, &crate::ID);
            let account_infos = &[system.clone(), account.clone(), base_account.clone()];
            invoke_signed_unchecked(&assign, account_infos, &[base_seeds])?;
        }

        Ok(account)
    }

    fn snapshot(&mut self) {
        // not supported on Solana
        // do nothing
    }

    fn revert(&mut self) {
        if self.panic_on_revert {
            panic_with_error!(Error::RevertWithSolanaCall);
        }
    }

    fn commit(&mut self) {
        // do nothing
    }
}
