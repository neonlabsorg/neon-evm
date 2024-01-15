use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::instruction::Instruction;
use solana_program::program::{invoke_signed_unchecked, invoke_unchecked};
use solana_program::rent::Rent;
use solana_program::system_program;
use solana_program::sysvar::Sysvar;

use crate::account::{AllocateResult, ContractAccount, StorageCell};
use crate::account_storage::SyncedAccountStorage;
use crate::config::{
    ACCOUNT_SEED_VERSION, STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT,
};
use crate::error::Result;
use crate::types::Address;

impl<'a> SyncedAccountStorage for crate::account_storage::ProgramAccountStorage<'a> {
    fn set_code(&mut self, address: Address, chain_id: u64, code: Vec<u8>) -> Result<()> {
        let rent = Rent::get()?;
        let result =
            ContractAccount::allocate(address, &code, &rent, &self.accounts, Some(&self.keys))?;

        if result != AllocateResult::Ready {
            return Err(crate::error::Error::AccountSpaceAllocationFailure);
        }

        ContractAccount::init(
            address,
            chain_id,
            0,
            &code,
            &self.accounts,
            Some(&self.keys),
        )?;

        Ok(())
    }

    fn set_storage(&mut self, address: Address, index: U256, value: [u8; 32]) -> Result<()> {
        const STATIC_STORAGE_LIMIT: U256 = U256::new(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u128);
        let rent = Rent::get()?;

        if index < STATIC_STORAGE_LIMIT {
            // Static Storage - Write into contract account
            let mut contract = self.contract_account(address)?;
            let index: usize = index.as_usize();
            contract.set_storage_value(index, &value);
        } else {
            // Infinite Storage - Write into separate account
            let cell_address = self.keys.storage_cell_address(&crate::ID, address, index);
            let account = self.accounts.get(cell_address.pubkey());
            if system_program::check_id(account.owner) {
                let (_, bump) = self.keys.contract_with_bump_seed(&crate::ID, address);
                let sign: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], address.as_bytes(), &[bump]];

                let mut storage = StorageCell::create(cell_address, 1, &self.accounts, sign)?;
                let mut cells = storage.cells_mut();

                assert_eq!(cells.len(), 1);
                cells[0].subindex = (index & 0xFF).as_u8();
                cells[0].value = value;
            } else {
                let mut storage = StorageCell::from_account(&crate::ID, account.clone())?;
                storage.update((index & 0xFF).as_u8(), &value)?;

                storage.sync_lamports(rent, &self.accounts)?;
            };
        }

        Ok(())
    }

    fn increment_nonce(&mut self, address: Address, chain_id: u64) -> Result<()> {
        let mut account = self.create_balance_account(address, chain_id)?;
        account.increment_nonce()
    }

    fn transfer(
        &mut self,
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()> {
        let mut source = self.balance_account(source, chain_id)?;
        let mut target = self.create_balance_account(target, chain_id)?;
        source.transfer(&mut target, value)
    }

    fn burn(&mut self, address: Address, chain_id: u64, value: U256) -> Result<()> {
        let mut account = self.create_balance_account(address, chain_id)?;
        account.burn(value)
    }

    fn execute_external_instruction(
        &mut self,
        instruction: Instruction,
        seeds: Vec<Vec<u8>>,
        _fee: u64,
    ) -> Result<()> {
        let seeds: Vec<&[u8]> = seeds.iter().map(|seed| &seed[..]).collect();

        let mut accounts_info = Vec::with_capacity(instruction.accounts.len() + 1);

        let program = self.accounts.get(&instruction.program_id).clone();
        accounts_info.push(program);

        for meta in &instruction.accounts {
            let account: AccountInfo<'a> =
                // TODO: Protect operator account from external calls
                if meta.pubkey == self.accounts.operator_key() {
                    self.accounts.operator_info().clone()
                } else {
                    self.accounts.get(&meta.pubkey).clone()
                };
            accounts_info.push(account);
        }

        let instruction = Instruction {
            program_id: instruction.program_id,
            accounts: instruction.accounts,
            data: instruction.data,
        };

        if !seeds.is_empty() {
            invoke_signed_unchecked(&instruction, &accounts_info, &[&seeds])?;
        } else {
            invoke_unchecked(&instruction, &accounts_info)?;
        }

        Ok(())
    }
}