use std::collections::HashMap;

use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::instruction::Instruction;
use solana_program::program::invoke_signed_unchecked;
use solana_program::rent::Rent;
use solana_program::system_program;
use solana_program::sysvar::Sysvar;

use crate::account::{AllocateResult, ContractAccount, StorageCell};
use crate::account::{BalanceAccount, StorageCellAddress};
use crate::account_storage::ProgramAccountStorage;
use crate::config::{PAYMENT_TO_TREASURE, STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT};
use crate::error::Result;
use crate::executor::Action;
use crate::types::Address;

impl<'a> ProgramAccountStorage<'a> {
    pub fn transfer_treasury_payment(&mut self) -> Result<()> {
        let system = self.accounts.system();
        let treasury = self.accounts.treasury();
        let operator = self.accounts.operator();

        system.transfer(operator, treasury, PAYMENT_TO_TREASURE)?;

        Ok(())
    }

    pub fn transfer_gas_payment(
        &mut self,
        origin: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()> {
        let (pubkey, _) = origin.find_balance_address(&crate::ID, chain_id);

        let source = self.accounts.get(&pubkey).clone();
        let mut source = BalanceAccount::from_account(&crate::ID, source, Some(origin))?;

        let target = self.accounts.operator_balance();

        source.transfer(target, value)
    }

    pub fn allocate(&mut self, actions: &[Action]) -> Result<AllocateResult> {
        let mut total_result = AllocateResult::Ready;

        let rent = Rent::get()?;

        for action in actions {
            if let Action::EvmSetCode { address, code, .. } = action {
                let result = ContractAccount::allocate(address, code, &rent, &self.accounts)?;
                if result == AllocateResult::NeedMore {
                    total_result = AllocateResult::NeedMore;
                }
            }
        }

        Ok(total_result)
    }

    pub fn apply_state_change(&mut self, actions: Vec<Action>) -> Result<()> {
        debug_print!("Applies begin");

        let mut storage = HashMap::with_capacity(16);

        for action in actions {
            match action {
                Action::Transfer {
                    source,
                    target,
                    chain_id,
                    value,
                } => {
                    let mut source = self.balance_account(source, chain_id)?;
                    let mut target = self.create_balance_account(target, chain_id)?;
                    source.transfer(&mut target, value)?;
                }
                Action::Burn {
                    source,
                    chain_id,
                    value,
                } => {
                    let mut account = self.create_balance_account(source, chain_id)?;
                    account.burn(value)?;
                }
                Action::EvmSetStorage {
                    address,
                    index,
                    value,
                } => {
                    storage
                        .entry(address)
                        .or_insert_with(|| HashMap::with_capacity(64))
                        .insert(index, value);
                }
                Action::EvmIncrementNonce { address, chain_id } => {
                    let mut account = self.create_balance_account(address, chain_id)?;
                    account.increment_nonce()?;
                }
                Action::EvmSetCode {
                    address,
                    chain_id,
                    code,
                } => {
                    ContractAccount::init(&address, chain_id, &code, &self.accounts)?;
                }
                Action::EvmSelfDestruct { address: _ } => {
                    // EIP-6780: SELFDESTRUCT only in the same transaction
                    // do nothing, balance was already transfered
                }
                Action::ExternalInstruction {
                    program_id,
                    accounts,
                    data,
                    seeds,
                    ..
                } => {
                    let seeds: Vec<&[u8]> = seeds.iter().map(|seed| &seed[..]).collect();

                    let mut accounts_info = Vec::with_capacity(accounts.len() + 1);

                    let program = self.accounts.get(&program_id).clone();
                    accounts_info.push(program);

                    for meta in &accounts {
                        let account: AccountInfo<'a> =
                            if meta.pubkey == self.accounts.operator_key() {
                                self.accounts.operator_info().clone()
                            } else {
                                self.accounts.get(&meta.pubkey).clone()
                            };
                        accounts_info.push(account);
                    }

                    let instruction = Instruction {
                        program_id,
                        accounts,
                        data,
                    };
                    invoke_signed_unchecked(&instruction, &accounts_info, &[&seeds])?;
                }
            }
        }

        self.apply_storage(storage)?;
        debug_print!("Applies done");

        Ok(())
    }

    fn apply_storage(&mut self, storage: HashMap<Address, HashMap<U256, [u8; 32]>>) -> Result<()> {
        const STATIC_STORAGE_LIMIT: U256 = U256::new(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u128);

        let rent = Rent::get()?;

        for (address, storage) in storage {
            let mut contract = self.contract_account(address)?;

            let mut infinite_values: HashMap<U256, HashMap<u8, [u8; 32]>> =
                HashMap::with_capacity(storage.len());

            for (index, value) in storage {
                if index < STATIC_STORAGE_LIMIT {
                    // Static Storage - Write into contract account
                    let index: usize = index.as_usize();
                    contract.set_storage_value(index, &value);
                } else {
                    // Infinite Storage - Write into separate account
                    let subindex = (index & 0xFF).as_u8();
                    let index = index & !U256::new(0xFF);

                    infinite_values
                        .entry(index)
                        .or_insert_with(|| HashMap::with_capacity(32))
                        .insert(subindex, value);
                }
            }

            for (index, values) in infinite_values {
                let cell = StorageCellAddress::new(&crate::ID, contract.pubkey(), &index);
                let account = self.accounts.get(cell.pubkey()).clone();

                if system_program::check_id(account.owner) {
                    let len = values.len();
                    let mut storage = StorageCell::create(address, index, len, &self.accounts)?;
                    let mut cells = storage.cells_mut();

                    assert_eq!(cells.len(), len);
                    for (cell, (subindex, value)) in cells.iter_mut().zip(values) {
                        cell.subindex = subindex;
                        cell.value = value;
                    }
                } else {
                    let mut storage = StorageCell::from_account(&crate::ID, account)?;
                    for (subindex, value) in values {
                        storage.update(subindex, &value)?;
                    }

                    storage.sync_lamports(rent, &self.accounts)?;
                };
            }
        }

        Ok(())
    }
}
