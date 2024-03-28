use std::sync::Arc;

use solana_accounts_db::transaction_results::{
    TransactionExecutionDetails, TransactionExecutionResult,
};
use solana_runtime::{
    bank::{Bank, TransactionSimulationResult},
    runtime_config::RuntimeConfig,
};
use solana_sdk::{
    account::Account,
    bpf_loader, bpf_loader_upgradeable,
    hash::Hash,
    pubkey::Pubkey,
    signature::Keypair,
    transaction::{
        MessageHash, SanitizedTransaction, TransactionVerificationMode, VersionedTransaction,
    },
};

use crate::rpc::Rpc;

mod error;
mod utils;

pub use error::Error;

pub struct SolanaSimulator {
    bank: Bank,
    runtime_config: Arc<RuntimeConfig>,
    payer: Keypair,
}

impl SolanaSimulator {
    pub async fn new(rpc: &impl Rpc) -> Result<Self, Error> {
        Self::new_with_config(rpc, RuntimeConfig::default()).await
    }

    pub async fn new_with_config(
        rpc: &impl Rpc,
        runtime_config: RuntimeConfig,
    ) -> Result<Self, Error> {
        let runtime_config = Arc::new(runtime_config);

        let info = utils::genesis_config_info(rpc, 1_000.0).await?;
        let payer = info.mint_keypair;

        let genesis_bank = Arc::new(Bank::new_with_paths(
            &info.genesis_config,
            Arc::clone(&runtime_config),
            Vec::default(),
            None,
            None,
            solana_accounts_db::accounts_index::AccountSecondaryIndexes::default(),
            solana_accounts_db::accounts_db::AccountShrinkThreshold::default(),
            false,
            None,
            None,
            Arc::default(),
        ));

        genesis_bank.set_capitalization();

        genesis_bank.fill_bank_with_ticks_for_tests();
        let mut bank = Bank::new_from_parent(
            Arc::clone(&genesis_bank),
            genesis_bank.collector_id(),
            genesis_bank.slot() + 1,
        );

        utils::sync_sysvar_accounts(rpc, &mut bank).await?;

        Ok(Self {
            bank,
            runtime_config,
            payer,
        })
    }

    pub async fn sync_accounts(&mut self, rpc: &impl Rpc, keys: &[Pubkey]) -> Result<(), Error> {
        let mut storable_accounts = vec![];

        let mut programdata_keys = vec![];

        let accounts = rpc.get_multiple_accounts(keys).await?;
        for (key, account) in keys.iter().zip(&accounts) {
            let Some(account) = account else {
                continue;
            };

            if account.executable && bpf_loader_upgradeable::check_id(&account.owner) {
                let programdata_address = utils::program_data_address(account)?;
                programdata_keys.push(programdata_address);
            }

            storable_accounts.push((key, account));
        }

        let mut programdata_accounts = rpc.get_multiple_accounts(&programdata_keys).await?;
        for (key, account) in programdata_keys.iter().zip(&mut programdata_accounts) {
            let Some(account) = account else {
                continue;
            };

            utils::reset_program_data_slot(account)?;
            storable_accounts.push((key, account));
        }

        self.set_multiple_accounts(&storable_accounts);

        Ok(())
    }

    fn bank(&self) -> &Bank {
        &self.bank
    }

    pub fn payer(&self) -> &Keypair {
        &self.payer
    }

    pub fn blockhash(&self) -> Hash {
        self.bank().last_blockhash()
    }

    pub fn slot(&self) -> u64 {
        self.bank().slot()
    }

    pub fn replace_blockhash(&mut self, blockhash: &Hash) {
        self.bank().register_recent_blockhash(blockhash);
    }

    pub fn set_program_account(&mut self, pubkey: &Pubkey, data: Vec<u8>) {
        let rent = self.bank().rent_collector().rent;
        let lamports = rent.minimum_balance(data.len());

        self.set_account(
            pubkey,
            &Account {
                lamports,
                data,
                owner: bpf_loader::ID,
                executable: true,
                rent_epoch: 0,
            },
        );
    }

    pub fn set_account(&mut self, pubkey: &Pubkey, account: &Account) {
        self.bank().store_account(pubkey, account);
    }

    pub fn set_multiple_accounts(&mut self, accounts: &[(&Pubkey, &Account)]) {
        let include_slot_in_hash = if self
            .bank()
            .feature_set
            .is_active(&solana_sdk::feature_set::account_hash_ignore_slot::id())
        {
            solana_accounts_db::accounts_db::IncludeSlotInHash::RemoveSlot
        } else {
            solana_accounts_db::accounts_db::IncludeSlotInHash::IncludeSlot
        };

        let storable_accounts = (self.slot(), accounts, include_slot_in_hash);
        self.bank().store_accounts(storable_accounts);
    }

    pub fn get_account(&self, pubkey: &Pubkey) -> Option<Account> {
        self.bank()
            .get_account_with_fixed_root(pubkey)
            .map(Account::from)
    }

    pub fn process_transaction(
        &mut self,
        tx: VersionedTransaction,
    ) -> Result<TransactionExecutionDetails, Error> {
        let mut result = self.process_multiple_transactions(vec![tx])?;

        Ok(result.remove(0))
    }

    pub fn process_multiple_transactions(
        &mut self,
        txs: Vec<VersionedTransaction>,
    ) -> Result<Vec<TransactionExecutionDetails>, Error> {
        let bank = self.bank();

        let mut sanitized_transactions = vec![];
        for tx in txs {
            let sanitized =
                bank.verify_transaction(tx, TransactionVerificationMode::FullVerification)?;
            sanitized_transactions.push(sanitized);
        }

        let batch = bank.prepare_sanitized_batch(&sanitized_transactions);

        let (
            solana_accounts_db::transaction_results::TransactionResults {
                execution_results, ..
            },
            ..,
        ) = bank.load_execute_and_commit_transactions(
            &batch,
            solana_sdk::clock::MAX_PROCESSING_AGE,
            false, // collect_balances
            true,  // enable_cpi_recording
            true,  // enable_log_recording
            true,  // enable_return_data_recording
            &mut solana_program_runtime::timings::ExecuteTimings::default(),
            self.runtime_config.log_messages_bytes_limit,
        );

        let mut result = Vec::with_capacity(execution_results.len());

        for execution_result in execution_results {
            match execution_result {
                TransactionExecutionResult::Executed { details, .. } => result.push(details),
                TransactionExecutionResult::NotExecuted(error) => return Err(error.into()),
            }
        }

        Ok(result)
    }

    pub fn simulate_transaction(
        &self,
        tx: VersionedTransaction,
    ) -> Result<TransactionSimulationResult, Error> {
        let sanitized =
            SanitizedTransaction::try_create(tx, MessageHash::Compute, None, self.bank())?;

        let simulation_result = self.bank().simulate_transaction_unchecked(sanitized);

        Ok(simulation_result)
    }

    pub fn simulate_legacy_transaction(
        &self,
        tx: solana_sdk::transaction::Transaction,
    ) -> Result<TransactionSimulationResult, Error> {
        let versioned_transaction = VersionedTransaction::from(tx);
        self.simulate_transaction(versioned_transaction)
    }
}
