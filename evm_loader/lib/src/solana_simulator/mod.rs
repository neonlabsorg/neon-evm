use std::{cell::RefCell, collections::HashMap, rc::Rc};

use mollusk_svm::{result::ContextResult, Mollusk, MolluskContext};
use solana_compute_budget::compute_budget::ComputeBudget;
use solana_log_collector::LogCollector;
use solana_sdk::{account::Account, instruction::Instruction, pubkey::Pubkey};
pub use utils::SyncState;

use crate::rpc::Rpc;

mod error;
pub use error::Error;

mod utils;

type InMemoryAccountStore = HashMap<Pubkey, Account>;

pub struct SolanaSimulator {
    mollusk_context: MolluskContext<InMemoryAccountStore>,
}

impl SolanaSimulator {
    pub async fn new(rpc: &impl Rpc) -> Result<Self, Error> {
        Self::new_with_config(rpc, ComputeBudget::default(), SyncState::Yes).await
    }

    pub async fn new_without_sync(rpc: &impl Rpc) -> Result<Self, Error> {
        Self::new_with_config(rpc, ComputeBudget::default(), SyncState::No).await
    }

    pub async fn new_with_config(
        rpc: &impl Rpc,
        compute_budget: ComputeBudget,
        sync_state: SyncState,
    ) -> Result<Self, Error> {
        let mut mollusk = Mollusk {
            compute_budget,
            ..Default::default()
        };

        if sync_state == SyncState::Yes {
            mollusk.sysvars = utils::download_sysvar_accounts(rpc).await?;
            mollusk.feature_set = utils::download_feature_set(rpc).await?;
        }

        let account_store = InMemoryAccountStore::default();
        Ok(Self {
            mollusk_context: mollusk.with_context(account_store),
        })
    }

    pub async fn sync_accounts(&mut self, rpc: &impl Rpc, keys: &[Pubkey]) -> Result<(), Error> {
        let keys = utils::filter_reserved_accounts(keys);

        let accounts = rpc.get_multiple_accounts(&keys).await?;
        for (pubkey, account) in keys.iter().zip(accounts.into_iter()) {
            let account = account.unwrap_or_default();

            if account.executable {
                let loader = account.owner;
                let elf = utils::extract_elf(rpc, account).await?;

                self.add_program(pubkey, &elf, &loader);
            } else {
                self.add_account(pubkey, account);
            }
        }

        Ok(())
    }

    pub fn add_account(&mut self, pubkey: &Pubkey, account: Account) {
        self.mollusk_context
            .account_store
            .borrow_mut()
            .insert(*pubkey, account);
    }

    pub fn add_program(&mut self, pubkey: &Pubkey, elf: &[u8], loader: &Pubkey) {
        self.mollusk_context
            .mollusk
            .add_program_with_elf_and_loader(pubkey, elf, loader);
    }

    #[must_use]
    pub fn get_account(&self, pubkey: &Pubkey) -> Account {
        self.mollusk_context
            .account_store
            .borrow()
            .get(pubkey)
            .cloned()
            .unwrap_or_default()
    }

    pub fn into_accounts(self) -> impl Iterator<Item = (Pubkey, Account)> {
        let store = Rc::into_inner(self.mollusk_context.account_store).unwrap();
        let store = RefCell::into_inner(store);
        store.into_iter()
    }

    #[must_use]
    pub fn process_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> (ContextResult, Vec<String>) {
        let log_collector = LogCollector::new_ref_with_limit(None);
        self.mollusk_context.mollusk.logger = Some(Rc::clone(&log_collector));

        let result = self.mollusk_context.process_instruction(instruction);
        let logs = log_collector.borrow().get_recorded_content().to_vec();

        (result, logs)
    }

    #[must_use]
    pub fn process_instruction_chain(
        &mut self,
        instructions: &[Instruction],
    ) -> (ContextResult, Vec<String>) {
        let log_collector = LogCollector::new_ref_with_limit(None);
        self.mollusk_context.mollusk.logger = Some(Rc::clone(&log_collector));

        let result = self.mollusk_context.process_instruction_chain(instructions);
        let logs = log_collector.borrow().get_recorded_content().to_vec();

        (result, logs)
    }
}
