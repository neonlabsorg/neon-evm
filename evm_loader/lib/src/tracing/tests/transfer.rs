use ethnum::U256;

use evm_loader::solana_program::pubkey::Pubkey;
use evm_loader::types::Address;

use crate::commands::emulate::EmulateResponse;
use crate::commands::get_config::BuildConfigSimulator;
use crate::rpc::Rpc;
use crate::tracing::tests::helpers;
use crate::tracing::tests::helpers::TestRpc;
use crate::tracing::tests::helpers::{rent_account, test_tracer};
use crate::tracing::TraceConfig;
use crate::types::TxParams;

#[tokio::test]
async fn test_trace_transfer_transaction() {
    trace_transfer_transaction(TraceConfig::default(), "{\"gas\":10000,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"STOP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]}]}", 3).await;
}

#[tokio::test]
async fn test_trace_state_diff_transfer_transaction() {
    trace_transfer_transaction(helpers::state_diff_trace_config(), "{\"output\":\"0x\",\"stateDiff\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":{\"*\":{\"from\":\"0x6c6a20ec9d08c1b590\",\"to\":\"0x6c68ae7dae54436b20\"}},\"nonce\":{\"*\":{\"from\":\"0x1\",\"to\":\"0x2\"}},\"code\":\"=\",\"storage\":{}},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":{\"*\":{\"from\":\"0x6c6cf6a1041aca0000\",\"to\":\"0x6c6e59e67c78540000\"}},\"nonce\":\"=\",\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}", 4).await;
}

#[tokio::test]
async fn test_trace_prestate_transfer_transaction() {
    trace_transfer_transaction(helpers::prestate_trace_config(), "{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":\"0x6c6a20ec9d08c1b590\",\"nonce\":1},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":\"0x6c6cf6a1041aca0000\",\"nonce\":0}}", 4).await;
}

#[tokio::test]
async fn test_trace_prestate_diff_mode_transfer_transaction() {
    trace_transfer_transaction(helpers::prestate_diff_mode_trace_config(), "{\"post\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":\"0x6c68ae7dae54436b20\",\"nonce\":2},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":\"0x6c6e59e67c78540000\"}},\"pre\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":\"0x6c6a20ec9d08c1b590\",\"nonce\":1},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":\"0x6c6cf6a1041aca0000\",\"nonce\":0}}}", 4).await;
}

// tx_hash: 0xa3c0a2d8f7519217775ca49e836cdfffec8cd1d16950553f3b41b580d10d44b7
async fn trace_transfer_transaction(
    trace_config: TraceConfig,
    expected_trace: &str,
    expected_solana_accounts: usize,
) {
    let tx = TxParams {
        from: Address::from_hex("0x4bbac480d466865807ec5e98ffdf429c170e2a4e").unwrap(),
        to: Some(Address::from_hex("0xf2418612ef70c2207da5d42511b7f58587ba27e3").unwrap()),
        value: Some(U256::from(100_000_000_000_000_000u64)),
        actual_gas_used: Some(U256::from(10_000u64)),
        gas_price: Some(U256::from(426_771_289_239u64)),
        gas_limit: Some(U256::from(30_000u64)),
        chain_id: Some(1234),
        ..TxParams::default()
    };

    let program_id = Pubkey::new_unique();

    test_tracer(
        program_id,
        trace_config,
        tx.clone(),
        transfer_transaction_rpc(program_id, tx),
        EmulateResponse {
            exit_status: "succeed".to_string(),
            result: vec![],
            steps_executed: 1,
            used_gas: 25_000,
            iterations: 3,
            solana_accounts: vec![],
        },
        expected_solana_accounts,
        expected_trace,
    )
    .await;
}

fn transfer_transaction_rpc(program_id: Pubkey, tx: TxParams) -> impl Rpc + BuildConfigSimulator {
    let chain_id = tx.chain_id.unwrap();

    TestRpc::new(
        [
            helpers::balance_account(
                &program_id,
                tx.from,
                chain_id,
                1,
                U256::from_str_hex("0x6c6a20ec9d08c1b590").unwrap(),
            ),
            helpers::balance_account(
                &program_id,
                tx.to.unwrap(),
                chain_id,
                0,
                U256::from_str_hex("0x6c6cf6a1041aca0000").unwrap(),
            ),
            rent_account(),
        ]
        .into(),
    )
}
