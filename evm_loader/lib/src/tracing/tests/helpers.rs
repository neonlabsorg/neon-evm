use crate::account_storage::EmulatorAccountStorage;
use crate::commands::get_config::{BuildConfigSimulator, ChainInfo, ConfigSimulator};
use crate::rpc::Rpc;
use crate::tracing::tracers::openeth::types::to_call_analytics;
use crate::tracing::tracers::prestate_tracer::tracer::PrestateTracerConfig;
use crate::tracing::TraceConfig;
use crate::NeonResult;
use async_trait::async_trait;
use ethnum::U256;
use evm_loader::account::BalanceAccount;
use evm_loader::solana_program::account_info::AccountInfo;
use evm_loader::solana_program::clock::{Slot, UnixTimestamp};
use evm_loader::solana_program::pubkey::Pubkey;
use evm_loader::solana_program::rent::Rent;
use evm_loader::types::Address;
use solana_client::client_error::Result as ClientResult;
use solana_client::rpc_response::{Response, RpcResponseContext, RpcResult};
use solana_sdk::account::Account;
use solana_sdk::commitment_config::CommitmentConfig;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct TestRpc {
    accounts: HashMap<Pubkey, Account>,
}

impl TestRpc {
    pub fn new(accounts: HashMap<Pubkey, Account>) -> Self {
        TestRpc { accounts }
    }
}

#[async_trait(?Send)]
impl Rpc for TestRpc {
    async fn get_account(&self, key: &Pubkey) -> RpcResult<Option<Account>> {
        Ok(Response {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: self.accounts.get(key).cloned(),
        })
    }

    async fn get_account_with_commitment(
        &self,
        _key: &Pubkey,
        _commitment: CommitmentConfig,
    ) -> RpcResult<Option<Account>> {
        unimplemented!()
    }

    async fn get_multiple_accounts(
        &self,
        _pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        Ok(vec![])
    }

    async fn get_block_time(&self, _slot: Slot) -> ClientResult<UnixTimestamp> {
        Ok(1234)
    }

    async fn get_slot(&self) -> ClientResult<Slot> {
        Ok(123)
    }
}

#[async_trait(?Send)]
impl BuildConfigSimulator for TestRpc {
    async fn build_config_simulator(&self, _program_id: Pubkey) -> NeonResult<ConfigSimulator> {
        unimplemented!()
    }
}

pub fn writable_account_info<'a>(key: &'a Pubkey, account: &'a mut Account) -> AccountInfo<'a> {
    AccountInfo {
        key,
        is_signer: false,
        is_writable: false, // TODO check why this has been removed
        lamports: Rc::new(RefCell::new(&mut account.lamports)),
        data: Rc::new(RefCell::new(&mut account.data)),
        owner: &account.owner,
        executable: account.executable,
        rent_epoch: account.rent_epoch,
    }
}

pub fn balance_account_with_data(
    program_id: &Pubkey,
    address: Address,
    chain_id: u64,
    trx_count: u64,
    balance: U256,
) -> (Pubkey, Account) {
    let mut account = Account::new(0, BalanceAccount::required_account_size(), program_id);
    let (pubkey, _) = address.find_balance_address(program_id, chain_id);

    let account_info = writable_account_info(&pubkey, &mut account);
    BalanceAccount::new(
        program_id,
        address,
        account_info,
        chain_id,
        trx_count,
        balance,
    )
    .unwrap();

    (pubkey, account)
}

pub fn rent_account() -> Account {
    let mut account = Account::new(0, 10000, &Pubkey::default());
    bincode::serialize_into(
        &mut account.data,
        &Rent {
            lamports_per_byte_year: 0,
            exemption_threshold: 0.0,
            burn_percent: 0,
        },
    )
    .unwrap();
    account
}

pub fn state_diff_trace_config() -> TraceConfig {
    TraceConfig {
        tracer: Some("openethereum".to_string()),
        tracer_config: Some(
            serde_json::to_value(to_call_analytics(&vec!["stateDiff".to_string()])).unwrap(),
        ),
        ..TraceConfig::default()
    }
}

pub fn prestate_trace_config() -> TraceConfig {
    TraceConfig {
        tracer: Some("prestateTracer".to_string()),
        tracer_config: None,
        ..TraceConfig::default()
    }
}

pub fn prestate_diff_mode_trace_config() -> TraceConfig {
    TraceConfig {
        tracer: Some("prestateTracer".to_string()),
        tracer_config: Some(
            serde_json::to_value(PrestateTracerConfig { diff_mode: true }).unwrap(),
        ),
        ..TraceConfig::default()
    }
}

pub async fn test_emulator_account_storage<T: Rpc + BuildConfigSimulator>(
    program_id: Pubkey,
    rpc: &T,
    chain_id: u64,
) -> EmulatorAccountStorage<T> {
    EmulatorAccountStorage::with_accounts(
        rpc,
        program_id,
        &[],
        Some(vec![ChainInfo {
            id: chain_id,
            name: "neon".to_string(),
            token: Default::default(),
        }]),
        None,
        None,
    )
    .await
    .unwrap()
}
