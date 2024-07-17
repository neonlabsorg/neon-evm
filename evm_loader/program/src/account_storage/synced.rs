use ethnum::U256;
use solana_program::instruction::Instruction;
use solana_program::program::{invoke_signed_unchecked, invoke_unchecked};
use solana_program::system_program;

use crate::account::{AllocateResult, ContractAccount, StorageCell};
use crate::account_storage::{SyncedAccountStorage, FAKE_OPERATOR};
use crate::config::{ACCOUNT_SEED_VERSION, STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT};
use crate::error::Result;
use crate::types::Address;

use super::{AccountStorage, ProgramAccountStorage};

impl<'a> SyncedAccountStorage for crate::account_storage::ProgramAccountStorage<'a> {
    fn set_code(&mut self, address: Address, chain_id: u64, code: Vec<u8>) -> Result<()> {
        let result = ContractAccount::allocate(
            address,
            &code,
            &self.rent,
            &self.accounts,
            Some(&self.keys),
        )?;

        if result != AllocateResult::Ready {
            return Err(crate::error::Error::AccountSpaceAllocationFailure);
        }

        ContractAccount::create(
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

        if index < STATIC_STORAGE_LIMIT {
            // Static Storage - Write into contract account
            let mut contract = self.contract_account(address)?;
            let index: usize = index.as_usize();
            contract.set_storage_value(index, &value);

            // Mark contract as modified
            // We can't increase the revision here because it might break the pointer to the contract code inside the evm.
            // TODO: After Account HEAP experiment, may be we could remove the Buffer magic
            self.synced_modified_contracts.insert(*contract.pubkey());
        } else {
            // Infinite Storage - Write into separate account
            let cell_address = self.keys.storage_cell_address(&crate::ID, address, index);
            let account = self.accounts.get(cell_address.pubkey());
            if system_program::check_id(account.owner) {
                let (_, bump) = self.keys.contract_with_bump_seed(&crate::ID, address);
                let sign: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], address.as_bytes(), &[bump]];

                let mut storage =
                    StorageCell::create(cell_address, 1, &self.accounts, sign, &self.rent)?;
                let mut cells = storage.cells_mut();

                assert_eq!(cells.len(), 1);
                cells[0].subindex = (index & 0xFF).as_u8();
                cells[0].value = value;
            } else {
                let mut storage = StorageCell::from_account(&crate::ID, account.clone())?;
                storage.update((index & 0xFF).as_u8(), &value)?;

                storage.sync_lamports(&self.rent, &self.accounts)?;
                storage.increment_revision(&self.rent, &self.accounts)?;
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
        let mut account = self.balance_account(address, chain_id)?;
        account.burn(value)
    }

    fn execute_external_instruction(
        &mut self,
        mut instruction: Instruction,
        seeds: Vec<Vec<Vec<u8>>>,
        _fee: u64,
        _emulated_internally: bool,
    ) -> Result<()> {
        let seeds = seeds
            .iter()
            .map(|s| s.iter().map(|s| s.as_slice()).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let seeds = seeds.iter().map(|s| s.as_slice()).collect::<Vec<_>>();

        let mut accounts_info = Vec::with_capacity(instruction.accounts.len() + 1);

        let program = self.accounts.get(&instruction.program_id).clone();
        accounts_info.push(program);

        for meta in &mut instruction.accounts {
            if meta.pubkey == FAKE_OPERATOR {
                meta.pubkey = self.accounts.operator_key();
            }
            let account = self.accounts.get(&meta.pubkey).clone();
            accounts_info.push(account);
        }

        let instruction = Instruction {
            program_id: instruction.program_id,
            accounts: instruction.accounts,
            data: instruction.data,
        };

        if !seeds.is_empty() {
            invoke_signed_unchecked(&instruction, &accounts_info, &seeds)?;
        } else {
            invoke_unchecked(&instruction, &accounts_info)?;
        }

        Ok(())
    }

    fn snapshot(&mut self) {}

    fn revert_snapshot(&mut self) {
        panic!("revert snapshot not implemented for ProgramAccountStorage");
    }

    fn commit_snapshot(&mut self) {}
}

impl<'a> ProgramAccountStorage<'a> {
    pub fn increment_revision_for_modified_contracts(&mut self) -> Result<()> {
        for pubkey in self.synced_modified_contracts.iter() {
            let account = self.accounts.get(pubkey);

            let mut contract = ContractAccount::from_account(&self.program_id(), account.clone())?;
            contract.increment_revision(&self.rent, &self.accounts)?;
        }

        self.synced_modified_contracts.clear();

        Ok(())
    }
}
