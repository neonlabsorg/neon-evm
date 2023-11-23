use std::rc::Rc;

use ethnum::U256;
use map_macro::hash_map;

use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::database::Database;
use evm_loader::executor::ExecutorState;
use evm_loader::solana_program::pubkey::Pubkey;
use evm_loader::types::Address;

use crate::commands::emulate::emulate_trx;
use crate::commands::get_config::BuildConfigSimulator;
use crate::commands::trace::into_traces;
use crate::rpc::Rpc;
use crate::tracing::tests::helpers;
use crate::tracing::tests::helpers::rent_account;
use crate::tracing::tests::helpers::TestRpc;
use crate::tracing::tracers::new_tracer;
use crate::tracing::TraceConfig;
use crate::types::TxParams;

#[tokio::test]
async fn test_trace_transfer_transaction() {
    trace_transfer_transaction(TraceConfig::default(), "{\"gas\":10000,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"STOP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]}]}").await;
}

#[tokio::test]
async fn test_trace_state_diff_transfer_transaction() {
    trace_transfer_transaction(helpers::state_diff_trace_config(), "{\"output\":\"0x\",\"stateDiff\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":{\"*\":{\"from\":\"0x6c6a20ec9d08c1b590\",\"to\":\"0x6c68ae7dae54436b20\"}},\"nonce\":{\"*\":{\"from\":\"0x1\",\"to\":\"0x2\"}},\"code\":\"=\",\"storage\":{}},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":{\"*\":{\"from\":\"0x6c6cf6a1041aca0000\",\"to\":\"0x6c6e59e67c78540000\"}},\"nonce\":\"=\",\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}").await;
}

#[tokio::test]
async fn test_trace_prestate_transfer_transaction() {
    trace_transfer_transaction(helpers::prestate_trace_config(), "{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":\"0x6c6a20ec9d08c1b590\",\"nonce\":1},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":\"0x6c6cf6a1041aca0000\",\"nonce\":0}}").await;
}

#[tokio::test]
async fn test_trace_prestate_diff_mode_transfer_transaction() {
    trace_transfer_transaction(helpers::prestate_diff_mode_trace_config(), "{\"post\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":\"0x6c68ae7dae54436b20\",\"nonce\":2},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":\"0x6c6e59e67c78540000\"}},\"pre\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":\"0x6c6a20ec9d08c1b590\",\"nonce\":1},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":\"0x6c6cf6a1041aca0000\",\"nonce\":0}}}").await;
}

// tx_hash: 0xa3c0a2d8f7519217775ca49e836cdfffec8cd1d16950553f3b41b580d10d44b7
async fn trace_transfer_transaction(trace_config: TraceConfig, expected_trace: &str) {
    let origin = Address::from_hex("0x4bbac480d466865807ec5e98ffdf429c170e2a4e").unwrap();
    let target = Address::from_hex("0xf2418612ef70c2207da5d42511b7f58587ba27e3").unwrap();
    let chain_id = 1234;

    let program_id = Pubkey::new_unique();

    let rpc = transfer_transaction_rpc(program_id, origin, target, chain_id);

    let mut test_account_storage =
        helpers::test_emulator_account_storage(program_id, &rpc, chain_id).await;

    let value = U256::from(100_000_000_000_000_000u64);

    let gas_used = Some(U256::from(10_000u64));

    let trx = transfer_tx_params(origin, target, value, gas_used).await;

    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let emulate_response = emulate_trx(trx, &mut backend, 1000, Some(Rc::clone(&tracer)))
        .await
        .unwrap();

    assert_eq!(emulate_response.exit_status, "succeed");
    assert_eq!(emulate_response.result, Vec::<u8>::new());
    assert_eq!(emulate_response.steps_executed, 1);
    assert_eq!(emulate_response.used_gas, 25_000);
    assert_eq!(emulate_response.iterations, 3);
    assert_eq!(emulate_response.solana_accounts.len(), 3);

    let result = into_traces(tracer, emulate_response);

    assert_eq!(serde_json::to_string(&result).unwrap(), expected_trace);

    assert_eq!(
        backend.balance(target, chain_id).await.unwrap()
            - test_account_storage
                .balance(target, chain_id)
                .await
                .unwrap(),
        value
    );
}

fn transfer_transaction_rpc(
    program_id: Pubkey,
    origin: Address,
    target: Address,
    chain_id: u64,
) -> impl Rpc + BuildConfigSimulator {
    let (origin_pubkey, origin_account) = helpers::balance_account_with_data(
        &program_id,
        origin,
        chain_id,
        1,
        U256::from_str_hex("0x6c6a20ec9d08c1b590").unwrap(),
    );

    let (target_pubkey, target_account) = helpers::balance_account_with_data(
        &program_id,
        target,
        chain_id,
        0,
        U256::from_str_hex("0x6c6cf6a1041aca0000").unwrap(),
    );

    TestRpc::new(hash_map! {
        origin_pubkey => origin_account,
        target_pubkey => target_account,
        solana_sdk::sysvar::rent::id() => rent_account()
    })
}

async fn transfer_tx_params(
    origin: Address,
    target: Address,
    value: U256,
    gas_used: Option<U256>,
) -> TxParams {
    TxParams {
        from: origin,
        to: Some(target),
        value: Some(value),
        gas_used,
        gas_price: Some(U256::from(426_771_289_239u64)),
        gas_limit: Some(U256::from(30_000u64)),
        ..TxParams::default()
    }
}
