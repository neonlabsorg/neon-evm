use crate::commands::emulate::{execute, EmulateResponse};
use crate::commands::get_config::{BuildConfigSimulator, ChainInfo, ConfigSimulator};
use crate::rpc::Rpc;
use crate::tracing::tracers::new_tracer;
use crate::tracing::tracers::openeth::types::to_call_analytics;
use crate::tracing::tracers::prestate_tracer::tracer::PrestateTracerConfig;
use crate::tracing::TraceConfig;
use crate::types::{EmulateRequest, TxParams};
use crate::NeonResult;
use async_trait::async_trait;
use ethnum::U256;
use evm_loader::account::BalanceAccount;
use evm_loader::solana_program::clock::{Slot, UnixTimestamp};
use evm_loader::solana_program::pubkey::Pubkey;
use evm_loader::solana_program::rent::Rent;
use evm_loader::types::Address;

use crate::account_storage::account_info;
use crate::tracing::tracers::call_tracer::CallTracerConfig;
use solana_client::client_error::Result as ClientResult;
use solana_client::rpc_response::{Response, RpcResponseContext, RpcResult};
use solana_sdk::account::Account;
use solana_sdk::commitment_config::CommitmentConfig;
use std::collections::HashMap;

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

pub fn balance_account(
    program_id: &Pubkey,
    address: Address,
    chain_id: u64,
    trx_count: u64,
    balance: U256,
) -> (Pubkey, Account) {
    let (pubkey, _) = address.find_balance_address(program_id, chain_id);

    let mut account = Account::new(0, BalanceAccount::required_account_size(), program_id);

    BalanceAccount::new(
        program_id,
        address,
        account_info(&pubkey, &mut account),
        chain_id,
        trx_count,
        balance,
    )
    .unwrap();

    (pubkey, account)
}

pub fn rent_account() -> (Pubkey, Account) {
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

    (solana_sdk::sysvar::rent::id(), account)
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

pub fn call_tracer_config() -> TraceConfig {
    TraceConfig {
        tracer: Some("callTracer".to_string()),
        tracer_config: Some(
            serde_json::to_value(CallTracerConfig {
                only_top_call: false,
                with_log: false,
            })
            .unwrap(),
        ),
        ..TraceConfig::default()
    }
}

pub async fn test_tracer(
    program_id: Pubkey,
    trace_config: TraceConfig,
    tx: TxParams,
    rpc: impl Rpc + BuildConfigSimulator,
    expected_emulate_response: EmulateResponse,
    expected_solana_accounts_len: usize,
    expected_traces: &str,
) {
    let (emulate_response, traces) = execute(
        &rpc,
        program_id,
        EmulateRequest {
            tx: tx.clone(),
            step_limit: Some(1_000),
            chains: Some(vec![ChainInfo {
                id: tx.chain_id.unwrap(),
                name: "neon".to_string(),
                token: Default::default(),
            }]),
            trace_config: None,
            accounts: vec![],
        },
        Some(new_tracer(&tx, trace_config).unwrap()),
    )
    .await
    .unwrap();

    assert_eq!(
        emulate_response.exit_status,
        expected_emulate_response.exit_status
    );
    assert_eq!(emulate_response.result, expected_emulate_response.result);
    assert_eq!(
        emulate_response.steps_executed,
        expected_emulate_response.steps_executed
    );
    assert_eq!(
        emulate_response.used_gas,
        expected_emulate_response.used_gas
    );
    assert_eq!(
        emulate_response.iterations,
        expected_emulate_response.iterations
    );
    assert_eq!(
        emulate_response.solana_accounts.len(),
        expected_solana_accounts_len
    );

    assert_eq!(
        serde_json::to_string(&traces.unwrap()).unwrap(),
        expected_traces.to_string()
    );
}
