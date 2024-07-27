use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub use error::Error;
use evm_loader::solana_program::bpf_loader_upgradeable::UpgradeableLoaderState;
use evm_loader::solana_program::clock::Slot;
use evm_loader::solana_program::loader_v4;
use evm_loader::solana_program::loader_v4::{LoaderV4State, LoaderV4Status};
use evm_loader::solana_program::message::SanitizedMessage;
use log::debug;
use solana_accounts_db::transaction_results::inner_instructions_list_from_instruction_trace;
use solana_bpf_loader_program::syscalls::create_program_runtime_environment_v1;
use solana_loader_v4_program::create_program_runtime_environment_v2;
use solana_program_runtime::compute_budget::ComputeBudget;
use solana_program_runtime::loaded_programs::{
    LoadProgramMetrics, LoadedProgram, LoadedProgramType, LoadedProgramsForTxBatch,
    ProgramRuntimeEnvironments,
};
use solana_program_runtime::log_collector::LogCollector;
use solana_program_runtime::message_processor::MessageProcessor;
use solana_program_runtime::sysvar_cache::SysvarCache;
use solana_program_runtime::timings::ExecuteTimings;
use solana_runtime::accounts::construct_instructions_account;
use solana_runtime::builtins::BUILTINS;
use solana_runtime::{bank::TransactionSimulationResult, runtime_config::RuntimeConfig};
use solana_sdk::account::{
    create_account_shared_data_with_fields, AccountSharedData, ReadableAccount,
    DUMMY_INHERITABLE_ACCOUNT_FIELDS, PROGRAM_OWNERS,
};
use solana_sdk::account_utils::StateMut;
use solana_sdk::address_lookup_table::error::AddressLookupError;
use solana_sdk::address_lookup_table::state::AddressLookupTable;
use solana_sdk::clock::Clock;
use solana_sdk::feature_set::FeatureSet;
use solana_sdk::fee_calculator::DEFAULT_TARGET_LAMPORTS_PER_SIGNATURE;
use solana_sdk::message::v0::{LoadedAddresses, MessageAddressTableLookup};
use solana_sdk::message::{AddressLoader, AddressLoaderError};
use solana_sdk::rent::Rent;
use solana_sdk::transaction::TransactionError;
use solana_sdk::transaction_context::{ExecutionRecord, IndexOfAccount, TransactionContext};
use solana_sdk::{
    account::Account,
    address_lookup_table, bpf_loader_upgradeable,
    hash::Hash,
    pubkey::Pubkey,
    signature::Keypair,
    sysvar::{Sysvar, SysvarId},
    transaction::{SanitizedTransaction, VersionedTransaction},
};
pub use utils::SyncState;

use crate::rpc::Rpc;

mod error;
mod utils;

pub struct SolanaSimulator {
    runtime_config: RuntimeConfig,
    feature_set: Arc<FeatureSet>,
    accounts_db: HashMap<Pubkey, AccountSharedData>,
    sysvar_cache: SysvarCache,
    payer: Keypair,
}

impl SolanaSimulator {
    pub async fn new(rpc: &impl Rpc) -> Result<Self, Error> {
        Self::new_with_config(rpc, RuntimeConfig::default(), SyncState::Yes).await
    }

    pub async fn new_without_sync(rpc: &impl Rpc) -> Result<Self, Error> {
        Self::new_with_config(rpc, RuntimeConfig::default(), SyncState::No).await
    }

    pub async fn new_with_config(
        rpc: &impl Rpc,
        runtime_config: RuntimeConfig,
        sync_state: SyncState,
    ) -> Result<Self, Error> {
        let mut feature_set = FeatureSet::all_enabled();

        if sync_state == SyncState::Yes {
            for feature in rpc.get_deactivated_solana_features().await? {
                feature_set.deactivate(&feature);
            }
        }

        let mut sysvar_cache = SysvarCache::default();

        sysvar_cache.set_rent(Rent::default());
        sysvar_cache.set_clock(Clock::default());

        if sync_state == SyncState::Yes {
            utils::sync_sysvar_accounts(rpc, &mut sysvar_cache).await?;
        }

        Ok(Self {
            runtime_config,
            feature_set: Arc::new(feature_set),
            accounts_db: HashMap::new(),
            sysvar_cache,
            payer: Keypair::new(),
        })
    }

    pub async fn sync_accounts(&mut self, rpc: &impl Rpc, keys: &[Pubkey]) -> Result<(), Error> {
        let mut storable_accounts: Vec<(&Pubkey, &Account)> = vec![];

        let mut programdata_keys = vec![];

        let mut accounts = rpc.get_multiple_accounts(keys).await?;
        for (key, account) in keys.iter().zip(&mut accounts) {
            let Some(account) = account else {
                continue;
            };

            if account.executable && bpf_loader_upgradeable::check_id(&account.owner) {
                let programdata_address = utils::program_data_address(account)?;
                debug!(
                    "program_data_account: program={key} programdata=address{programdata_address}"
                );
                programdata_keys.push(programdata_address);
            }

            if account.owner == address_lookup_table::program::id() {
                utils::reset_alt_slot(account).map_err(|_| Error::InvalidALT)?;
            }

            storable_accounts.push((key, account));
        }

        let mut programdata_accounts = rpc.get_multiple_accounts(&programdata_keys).await?;
        for (key, account) in programdata_keys.iter().zip(&mut programdata_accounts) {
            let Some(account) = account else {
                continue;
            };

            debug!("program_data_account: key={key} account={account:?}");
            utils::reset_program_data_slot(account)?;
            storable_accounts.push((key, account));
        }

        self.set_multiple_accounts(&storable_accounts);

        Ok(())
    }

    #[must_use]
    pub const fn payer(&self) -> &Keypair {
        &self.payer
    }

    #[must_use]
    pub fn blockhash(&self) -> Hash {
        Hash::new_unique()
    }

    pub fn slot(&self) -> Result<u64, Error> {
        let clock = self.sysvar_cache.get_clock()?;
        Ok(clock.slot)
    }

    fn replace_sysvar_account<S>(&mut self, sysvar: &S)
    where
        S: Sysvar + SysvarId,
    {
        let old_account = self.accounts_db.get(&S::id());
        let inherit = old_account.map_or(DUMMY_INHERITABLE_ACCOUNT_FIELDS, |a| {
            (a.lamports(), a.rent_epoch())
        });

        let account = create_account_shared_data_with_fields(sysvar, inherit);
        self.accounts_db.insert(S::id(), account);
    }

    pub fn set_clock(&mut self, clock: Clock) {
        self.replace_sysvar_account(&clock);
        self.sysvar_cache.set_clock(clock);
    }

    pub fn set_multiple_accounts(&mut self, accounts: &[(&Pubkey, &Account)]) {
        for (pubkey, account) in accounts {
            self.accounts_db
                .insert(**pubkey, AccountSharedData::from((*account).clone()));
        }
    }

    #[must_use]
    pub fn get_shared_account(&self, pubkey: &Pubkey) -> Option<AccountSharedData> {
        self.accounts_db.get(pubkey).cloned()
    }

    pub fn sanitize_transaction(
        &self,
        tx: VersionedTransaction,
        verify: bool,
    ) -> Result<SanitizedTransaction, Error> {
        let sanitized_tx = {
            let size = bincode::serialized_size(&tx)?;
            if verify && (size > solana_sdk::packet::PACKET_DATA_SIZE as u64) {
                return Err(TransactionError::SanitizeFailure.into());
            }

            let message_hash = if verify {
                tx.verify_and_hash_message()?
            } else {
                tx.message.hash()
            };

            SanitizedTransaction::try_create(tx, message_hash, None, self)
        }?;

        if verify {
            sanitized_tx.verify_precompiles(&self.feature_set)?;
        }

        Ok(sanitized_tx)
    }

    pub fn process_transaction(
        &self,
        blockhash: Hash,
        tx: &SanitizedTransaction,
    ) -> Result<TransactionSimulationResult, Error> {
        let mut transaction_accounts = Vec::new();
        for key in tx.message().account_keys().iter() {
            let account = if solana_sdk::sysvar::instructions::check_id(key) {
                construct_instructions_account(tx.message())
            } else {
                self.accounts_db.get(key).cloned().unwrap_or_default()
            };
            transaction_accounts.push((*key, account));
        }

        let program_indices = Self::build_program_indices(tx, &mut transaction_accounts);

        let compute_budget = self.runtime_config.compute_budget.unwrap_or_default();
        let rent: Arc<Rent> = self.sysvar_cache.get_rent()?;
        let clock: Arc<Clock> = self.sysvar_cache.get_clock()?;

        let lamports_before_tx =
            transaction_accounts_lamports_sum(&transaction_accounts, tx.message()).unwrap_or(0);

        let mut transaction_context = TransactionContext::new(
            transaction_accounts,
            *rent,
            compute_budget.max_invoke_stack_height,
            compute_budget.max_instruction_trace_length,
        );

        let loaded_programs = self.load_programs(tx, &compute_budget, &clock);

        let mut modified_programs = LoadedProgramsForTxBatch::new(
            clock.slot,
            loaded_programs.environments.clone(),
            loaded_programs.upcoming_environments.clone(),
            loaded_programs.latest_root_epoch,
        );

        let log_collector =
            LogCollector::new_ref_with_limit(self.runtime_config.log_messages_bytes_limit);

        let mut units_consumed = 0u64;

        let mut status = MessageProcessor::process_message(
            tx.message(),
            &program_indices,
            &mut transaction_context,
            Some(Rc::clone(&log_collector)),
            &loaded_programs,
            &mut modified_programs,
            Arc::clone(&self.feature_set),
            compute_budget,
            &mut ExecuteTimings::default(),
            &self.sysvar_cache,
            blockhash,
            DEFAULT_TARGET_LAMPORTS_PER_SIGNATURE / 2,
            &mut units_consumed,
        );

        let inner_instructions = Some(inner_instructions_list_from_instruction_trace(
            &transaction_context,
        ));

        let ExecutionRecord {
            accounts,
            return_data,
            touched_account_count: _touched_account_count,
            accounts_resize_delta: _accounts_resize_delta,
        } = transaction_context.into();

        if status.is_ok()
            && transaction_accounts_lamports_sum(&accounts, tx.message())
                .filter(|lamports_after_tx| lamports_before_tx == *lamports_after_tx)
                .is_none()
        {
            status = Err(TransactionError::UnbalancedTransaction);
        }

        let logs = Rc::try_unwrap(log_collector)
            .map(|log_collector| log_collector.into_inner().into_messages())
            .ok()
            .unwrap();

        let return_data = if return_data.data.is_empty() {
            None
        } else {
            Some(return_data)
        };

        Ok(TransactionSimulationResult {
            result: status,
            logs,
            post_simulation_accounts: accounts,
            units_consumed,
            return_data,
            inner_instructions,
        })
    }

    #[allow(clippy::cast_possible_truncation)]
    fn build_program_indices(
        tx: &SanitizedTransaction,
        transaction_accounts: &mut Vec<(Pubkey, AccountSharedData)>,
    ) -> Vec<Vec<IndexOfAccount>> {
        let builtins_start_index = transaction_accounts.len();
        tx.message()
            .instructions()
            .iter()
            .map(|instruction| {
                let mut account_indices: Vec<IndexOfAccount> = Vec::with_capacity(2);

                let program_index = instruction.program_id_index as usize;
                let (program_id, program_account) = &transaction_accounts[program_index];

                if solana_sdk::native_loader::check_id(program_id) {
                    return account_indices;
                }

                account_indices.insert(0, program_index as IndexOfAccount);

                let owner = program_account.owner();
                if solana_sdk::native_loader::check_id(owner) {
                    return account_indices;
                }

                if let Some(owner_index) = transaction_accounts[builtins_start_index..]
                    .iter()
                    .position(|(key, _)| key == owner)
                {
                    let owner_index = owner_index + builtins_start_index;
                    account_indices.insert(0, owner_index as IndexOfAccount);
                } else {
                    let _builtin = BUILTINS
                        .iter()
                        .find(|builtin| builtin.program_id == *owner)
                        .unwrap();

                    let owner_account =
                        AccountSharedData::new(100, 100, &solana_sdk::native_loader::id());
                    transaction_accounts.push((*owner, owner_account));

                    let owner_index = transaction_accounts.len() - 1;
                    account_indices.insert(0, owner_index as IndexOfAccount);
                }

                account_indices
            })
            .collect()
    }

    fn load_programs(
        &self,
        tx: &SanitizedTransaction,
        compute_budget: &ComputeBudget,
        clock: &Arc<Clock>,
    ) -> LoadedProgramsForTxBatch {
        let program_runtime_environments = ProgramRuntimeEnvironments {
            program_runtime_v1: Arc::new(
                create_program_runtime_environment_v1(
                    &self.feature_set,
                    compute_budget,
                    true,
                    true,
                )
                .unwrap(),
            ),
            program_runtime_v2: Arc::new(create_program_runtime_environment_v2(
                compute_budget,
                true,
            )),
        };

        let mut loaded_programs = LoadedProgramsForTxBatch::new(
            clock.slot,
            program_runtime_environments.clone(),
            None,
            clock.epoch,
        );

        tx.message().account_keys().iter().for_each(|key| {
            if loaded_programs.find(key).is_none() {
                let account = self.accounts_db.get(key).cloned().unwrap_or_default();
                if PROGRAM_OWNERS.iter().any(|owner| account.owner() == owner) {
                    let mut load_program_metrics = LoadProgramMetrics {
                        program_id: key.to_string(),
                        ..LoadProgramMetrics::default()
                    };
                    let loaded_program = match self.load_program_accounts(account) {
                        ProgramAccountLoadResult::InvalidAccountData => {
                            LoadedProgram::new_tombstone(0, LoadedProgramType::Closed)
                        }

                        ProgramAccountLoadResult::ProgramOfLoaderV1orV2(program_account) => {
                            LoadedProgram::new(
                                program_account.owner(),
                                program_runtime_environments.program_runtime_v1.clone(),
                                0,
                                0,
                                None,
                                program_account.data(),
                                program_account.data().len(),
                                &mut load_program_metrics,
                            )
                            .unwrap()
                        }

                        ProgramAccountLoadResult::ProgramOfLoaderV3(
                            program_account,
                            programdata_account,
                            _slot,
                        ) => {
                            let programdata = programdata_account
                                .data()
                                .get(UpgradeableLoaderState::size_of_programdata_metadata()..)
                                .unwrap();
                            LoadedProgram::new(
                                program_account.owner(),
                                program_runtime_environments.program_runtime_v1.clone(),
                                0,
                                0,
                                None,
                                programdata,
                                program_account
                                    .data()
                                    .len()
                                    .saturating_add(programdata_account.data().len()),
                                &mut load_program_metrics,
                            )
                            .unwrap()
                        }

                        ProgramAccountLoadResult::ProgramOfLoaderV4(program_account, _slot) => {
                            let elf_bytes = program_account
                                .data()
                                .get(LoaderV4State::program_data_offset()..)
                                .unwrap();
                            LoadedProgram::new(
                                program_account.owner(),
                                program_runtime_environments.program_runtime_v2.clone(),
                                0,
                                0,
                                None,
                                elf_bytes,
                                program_account.data().len(),
                                &mut load_program_metrics,
                            )
                            .unwrap()
                        }
                    };
                    loaded_programs.replenish(*key, Arc::new(loaded_program));
                }
            }
        });

        for builtin in BUILTINS {
            // create_loadable_account_with_fields
            let program = LoadedProgram::new_builtin(0, builtin.name.len(), builtin.entrypoint);
            loaded_programs.replenish(builtin.program_id, Arc::new(program));
        }

        loaded_programs
    }

    pub fn simulate_legacy_transaction(
        &self,
        tx: solana_sdk::transaction::Transaction,
    ) -> Result<TransactionSimulationResult, Error> {
        let versioned_transaction = VersionedTransaction::from(tx);
        self.process_transaction(
            *versioned_transaction.message.recent_blockhash(),
            &self.sanitize_transaction(versioned_transaction, false)?,
        )
    }

    fn load_program_accounts(
        &self,
        program_account: AccountSharedData,
    ) -> ProgramAccountLoadResult {
        debug_assert!(solana_bpf_loader_program::check_loader_id(
            program_account.owner()
        ));

        if loader_v4::check_id(program_account.owner()) {
            return solana_loader_v4_program::get_state(program_account.data())
                .ok()
                .and_then(|state| {
                    (!matches!(state.status, LoaderV4Status::Retracted)).then_some(state.slot)
                })
                .map_or(ProgramAccountLoadResult::InvalidAccountData, |slot| {
                    ProgramAccountLoadResult::ProgramOfLoaderV4(program_account, slot)
                });
        }

        if !bpf_loader_upgradeable::check_id(program_account.owner()) {
            return ProgramAccountLoadResult::ProgramOfLoaderV1orV2(program_account);
        }

        if let Ok(UpgradeableLoaderState::Program {
            programdata_address,
        }) = program_account.state()
        {
            if let Some(programdata_account) = self.accounts_db.get(&programdata_address).cloned() {
                if let Ok(UpgradeableLoaderState::ProgramData {
                    slot,
                    upgrade_authority_address: _,
                }) = programdata_account.state()
                {
                    return ProgramAccountLoadResult::ProgramOfLoaderV3(
                        program_account,
                        programdata_account,
                        slot,
                    );
                }
            }
        }
        ProgramAccountLoadResult::InvalidAccountData
    }
}

enum ProgramAccountLoadResult {
    InvalidAccountData,
    ProgramOfLoaderV1orV2(AccountSharedData),
    ProgramOfLoaderV3(AccountSharedData, AccountSharedData, Slot),
    ProgramOfLoaderV4(AccountSharedData, Slot),
}

fn transaction_accounts_lamports_sum(
    accounts: &[(Pubkey, AccountSharedData)],
    message: &SanitizedMessage,
) -> Option<u128> {
    let mut lamports_sum = 0u128;
    for i in 0..message.account_keys().len() {
        let (_, account) = accounts.get(i)?;
        lamports_sum = lamports_sum.checked_add(u128::from(account.lamports()))?;
    }
    Some(lamports_sum)
}

impl AddressLoader for &SolanaSimulator {
    fn load_addresses(
        self,
        lookups: &[MessageAddressTableLookup],
    ) -> Result<LoadedAddresses, AddressLoaderError> {
        let loaded_addresses = lookups
            .iter()
            .map(|address_table_lookup| {
                let table_account = self
                    .get_shared_account(&address_table_lookup.account_key)
                    .ok_or(AddressLookupError::LookupTableAccountNotFound)?;

                if table_account.owner() != &address_lookup_table::program::id() {
                    return Err(AddressLookupError::InvalidAccountOwner);
                }

                let current_slot = self
                    .slot()
                    .map_err(|_| AddressLookupError::LookupTableAccountNotFound)?;

                let slot_hashes = self
                    .sysvar_cache
                    .get_slot_hashes()
                    .map_err(|_| AddressLookupError::LookupTableAccountNotFound)?;

                let lookup_table = AddressLookupTable::deserialize(table_account.data())
                    .map_err(|_| AddressLookupError::InvalidAccountData)?;

                Ok(LoadedAddresses {
                    writable: lookup_table.lookup(
                        current_slot,
                        &address_table_lookup.writable_indexes,
                        &slot_hashes,
                    )?,
                    readonly: lookup_table.lookup(
                        current_slot,
                        &address_table_lookup.readonly_indexes,
                        &slot_hashes,
                    )?,
                })
            })
            .collect::<Result<_, AddressLookupError>>()?;

        Ok(loaded_addresses)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_hex_encode_tx() {
        let bytes = [
            1, 58, 23, 68, 33, 87, 114, 44, 125, 7, 236, 250, 189, 152, 80, 109, 13, 162, 107, 101,
            124, 216, 66, 80, 213, 40, 53, 51, 182, 30, 255, 233, 81, 173, 129, 169, 64, 34, 99,
            244, 26, 97, 234, 36, 224, 159, 246, 251, 59, 49, 38, 37, 93, 186, 243, 244, 21, 130,
            128, 72, 105, 242, 160, 60, 7, 1, 0, 12, 17, 231, 21, 48, 207, 152, 236, 233, 187, 223,
            100, 82, 93, 7, 113, 26, 194, 124, 70, 245, 140, 6, 215, 63, 170, 178, 46, 130, 201,
            93, 40, 215, 178, 87, 173, 47, 151, 71, 81, 72, 73, 80, 156, 165, 181, 37, 178, 136,
            180, 35, 74, 234, 62, 83, 114, 123, 149, 139, 225, 217, 56, 147, 131, 206, 45, 105,
            208, 134, 80, 73, 140, 123, 117, 252, 233, 38, 133, 137, 99, 77, 55, 167, 149, 85, 24,
            36, 45, 148, 20, 106, 29, 21, 63, 99, 229, 104, 209, 135, 106, 102, 83, 227, 22, 170,
            173, 106, 135, 14, 233, 81, 75, 157, 216, 252, 53, 197, 36, 130, 119, 213, 126, 95, 12,
            254, 107, 149, 132, 193, 66, 216, 86, 105, 62, 222, 21, 157, 45, 17, 10, 178, 124, 189,
            91, 143, 219, 219, 104, 133, 196, 14, 129, 66, 215, 247, 28, 36, 71, 221, 69, 99, 94,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 3, 6, 70, 111, 229, 33, 23, 50, 255, 236, 173, 186, 114, 195, 155, 231, 188,
            140, 229, 187, 197, 247, 18, 107, 44, 67, 155, 58, 64, 0, 0, 0, 7, 215, 243, 216, 48,
            133, 75, 3, 197, 149, 83, 168, 150, 114, 90, 186, 70, 253, 177, 48, 225, 211, 142, 191,
            241, 13, 77, 252, 188, 202, 215, 228, 40, 226, 52, 125, 171, 215, 127, 195, 1, 133, 3,
            55, 38, 152, 124, 161, 200, 77, 29, 38, 158, 98, 147, 173, 205, 87, 50, 86, 244, 38,
            253, 251, 60, 0, 57, 43, 120, 125, 56, 168, 83, 209, 36, 5, 118, 52, 196, 60, 113, 51,
            198, 18, 70, 29, 116, 254, 177, 127, 66, 72, 21, 82, 134, 192, 90, 72, 243, 77, 170,
            80, 23, 118, 170, 39, 23, 58, 58, 106, 22, 11, 26, 111, 175, 251, 186, 179, 49, 60, 52,
            157, 46, 126, 243, 174, 139, 107, 91, 173, 150, 43, 91, 46, 211, 20, 76, 212, 62, 196,
            7, 39, 240, 9, 4, 65, 17, 249, 203, 197, 140, 181, 38, 196, 213, 83, 115, 120, 145, 11,
            94, 103, 100, 49, 70, 40, 205, 74, 224, 203, 66, 252, 113, 38, 143, 168, 244, 184, 32,
            23, 8, 184, 93, 133, 24, 57, 107, 236, 123, 117, 13, 177, 115, 140, 234, 18, 14, 113,
            230, 247, 22, 144, 89, 29, 2, 140, 149, 150, 43, 254, 247, 11, 251, 18, 99, 69, 131,
            32, 152, 113, 83, 125, 6, 8, 196, 54, 180, 68, 255, 57, 153, 93, 12, 42, 185, 242, 64,
            126, 36, 91, 71, 227, 19, 94, 244, 179, 157, 76, 97, 249, 91, 53, 39, 250, 133, 28,
            197, 116, 21, 173, 61, 163, 236, 185, 166, 242, 81, 61, 9, 179, 63, 76, 24, 114, 82,
            18, 182, 138, 185, 121, 251, 101, 108, 251, 132, 89, 201, 201, 238, 34, 115, 103, 121,
            227, 23, 147, 27, 162, 42, 67, 149, 80, 211, 223, 101, 182, 167, 128, 216, 67, 155, 21,
            245, 228, 13, 178, 54, 172, 119, 171, 72, 106, 157, 37, 107, 127, 172, 209, 12, 110,
            173, 6, 145, 14, 187, 127, 145, 195, 53, 103, 119, 182, 129, 49, 170, 8, 237, 99, 87,
            32, 152, 64, 4, 6, 0, 9, 3, 2, 0, 0, 0, 0, 0, 0, 0, 6, 0, 5, 1, 0, 0, 4, 0, 6, 0, 5, 2,
            192, 92, 21, 0, 9, 15, 2, 0, 3, 4, 5, 7, 8, 1, 10, 11, 12, 13, 14, 15, 16, 122, 52, 14,
            0, 0, 0, 88, 49, 0, 0, 12, 0, 0, 0, 248, 107, 1, 132, 119, 53, 148, 0, 132, 9, 61, 92,
            128, 148, 163, 222, 235, 37, 106, 70, 13, 34, 201, 224, 71, 134, 58, 94, 231, 159, 237,
            188, 62, 73, 128, 132, 52, 103, 185, 80, 130, 1, 2, 160, 244, 180, 133, 103, 74, 93,
            21, 159, 64, 57, 219, 13, 44, 152, 135, 226, 169, 32, 159, 38, 122, 70, 119, 143, 47,
            187, 8, 70, 187, 84, 246, 50, 160, 37, 44, 189, 121, 39, 163, 64, 35, 212, 96, 114,
            159, 112, 19, 137, 242, 122, 153, 40, 66, 91, 58, 177, 212, 56, 211, 173, 170, 83, 13,
            176, 85,
        ];
        eprintln!("{}", hex::encode(bytes));
        // assert_eq!(true, false);
    }

    #[test]
    fn test_hex_encode_v0_tx() {
        let bytes = [
            1, 195, 51, 167, 150, 247, 55, 53, 248, 87, 145, 226, 107, 103, 143, 215, 85, 62, 100,
            128, 254, 136, 249, 2, 68, 10, 226, 181, 117, 197, 116, 123, 10, 25, 188, 7, 130, 56,
            16, 164, 148, 238, 144, 40, 1, 29, 213, 36, 243, 153, 42, 247, 46, 74, 245, 246, 187,
            202, 223, 137, 176, 212, 204, 198, 6, 128, 1, 0, 3, 4, 129, 250, 211, 63, 136, 192,
            175, 63, 138, 71, 161, 34, 135, 75, 55, 3, 149, 61, 84, 23, 252, 239, 203, 80, 170,
            175, 222, 166, 136, 217, 173, 126, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 6, 70, 111, 229, 33, 23, 50, 255, 236,
            173, 186, 114, 195, 155, 231, 188, 140, 229, 187, 197, 247, 18, 107, 44, 67, 155, 58,
            64, 0, 0, 0, 60, 0, 57, 43, 120, 125, 56, 168, 83, 209, 36, 5, 118, 52, 196, 60, 113,
            51, 198, 18, 70, 29, 116, 254, 177, 127, 66, 72, 21, 82, 134, 192, 120, 25, 149, 73,
            158, 141, 39, 109, 12, 97, 90, 180, 212, 130, 149, 30, 62, 56, 197, 80, 116, 62, 58, 5,
            27, 10, 213, 206, 39, 141, 199, 83, 4, 2, 0, 9, 3, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 5, 1,
            0, 0, 4, 0, 2, 0, 5, 2, 192, 92, 21, 0, 3, 32, 24, 0, 29, 17, 1, 6, 28, 26, 33, 16, 18,
            31, 8, 12, 11, 25, 13, 32, 30, 15, 14, 4, 10, 22, 9, 27, 23, 21, 5, 19, 20, 7, 5, 51,
            29, 0, 0, 0, 1, 35, 227, 136, 54, 194, 46, 23, 104, 208, 155, 167, 222, 101, 18, 44,
            98, 220, 121, 194, 180, 176, 19, 46, 226, 47, 225, 188, 124, 123, 18, 197, 81, 26, 0,
            1, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 20, 21, 22, 24, 25, 26, 27,
            28, 29, 4, 2, 3, 18, 23,
        ];
        eprintln!("{}", hex::encode(bytes));
        // assert_eq!(true, false);
    }
}
