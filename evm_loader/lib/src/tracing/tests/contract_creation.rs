use std::rc::Rc;

use ethnum::U256;
use map_macro::hash_map;

use evm_loader::executor::ExecutorState;
use evm_loader::solana_program::pubkey::Pubkey;
use evm_loader::types::Address;

use crate::commands::emulate::emulate_trx;
use crate::commands::trace::into_traces;
use crate::tracing::tests::helpers;
use crate::tracing::tests::helpers::rent_account;
use crate::tracing::tests::helpers::TestRpc;
use crate::tracing::tracers::new_tracer;
use crate::tracing::TraceConfig;
use crate::types::TxParams;

#[tokio::test]
async fn test_trace_contract_creation() {
    trace_contract_creation(TraceConfig::default(), "{\"gas\":19010800,\"failed\":false,\"returnValue\":\"608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":21,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\"]},{\"pc\":25,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\"]},{\"pc\":27,\"op\":\"CODECOPY\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\",\"0x0\"]},{\"pc\":28,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":30,\"op\":\"RETURN\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x0\"]}]}").await;
    // TODO: Uncomment this if fix for NDEV-2329 is rejected
    // trace_contract_creation(TraceConfig::default(), "{\"gas\":19010800,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":21,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\"]},{\"pc\":25,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\"]},{\"pc\":27,\"op\":\"CODECOPY\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\",\"0x0\"]},{\"pc\":28,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":30,\"op\":\"RETURN\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x0\"]}]}").await;
}

#[tokio::test]
async fn test_trace_state_diff_contract_creation() {
    trace_contract_creation(helpers::state_diff_trace_config(), "{\"output\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":{\"+\":\"0x0\"},\"nonce\":{\"+\":\"0x1\"},\"code\":{\"+\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\"},\"storage\":{}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4c8447cf1ec3d2090\",\"to\":\"0x4693551a823c1c310\"}},\"nonce\":{\"*\":{\"from\":\"0x5\",\"to\":\"0x6\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}").await;
    // TODO: Uncomment this if fix for NDEV-2329 is rejected
    // trace_contract_creation(helpers::state_diff_trace_config(), "{\"output\":\"0x\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":{\"+\":\"0x0\"},\"nonce\":{\"+\":\"0x1\"},\"code\":{\"+\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\"},\"storage\":{}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4c8447cf1ec3d2090\",\"to\":\"0x4693551a823c1c310\"}},\"nonce\":{\"*\":{\"from\":\"0x5\",\"to\":\"0x6\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}").await;
}

#[tokio::test]
async fn test_trace_prestate_contract_creation() {
    trace_contract_creation(helpers::prestate_trace_config(), "{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":\"0x0\",\"nonce\":0},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":\"0x4c8447cf1ec3d2090\",\"nonce\":5}}").await;
}

#[tokio::test]
async fn test_trace_prestate_diff_mode_contract_creation() {
    trace_contract_creation(helpers::prestate_diff_mode_trace_config(), "{\"post\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"code\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"nonce\":1},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":\"0x4693551a823c1c310\",\"nonce\":6}},\"pre\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":\"0x0\",\"nonce\":0},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":\"0x4c8447cf1ec3d2090\",\"nonce\":5}}}").await;
}

// tx_hash: 0xe888205d8f3f504d45da558ea0aea42a35130dad77f5e34c940acce3ca9182a8
async fn trace_contract_creation(trace_config: TraceConfig, expected_trace: &str) {
    let gas_used = Some(U256::from(19_010_800u64));
    let chain_id = 1234;

    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let program_id = Pubkey::new_unique();

    let from = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();

    let (from_pubkey, from_account) = helpers::balance_account_with_data(
        &program_id,
        from,
        chain_id,
        5,
        U256::from_str_hex("0x4c8447cf1ec3d2090").unwrap(),
    );

    let rpc = TestRpc::new(hash_map! {
        from_pubkey => from_account,
        solana_sdk::sysvar::rent::id() => rent_account()
    });

    let mut test_account_storage =
        helpers::test_emulator_account_storage(program_id, &rpc, chain_id).await;

    let code = hex::decode("608060405234801561001057600080fd5b506101e3806100206000396000f3fe608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033").unwrap();

    let tx_params = TxParams {
        from,
        data: Some(code.clone()),
        gas_limit: Some(U256::from(19_035_800u64)),
        gas_used,
        gas_price: Some(U256::from(360_307_885_736u64)),
        ..TxParams::default()
    };

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let emulate_response = emulate_trx(tx_params, &mut backend, 1000, Some(Rc::clone(&tracer)))
        .await
        .unwrap();

    assert_eq!(emulate_response.exit_status, "succeed");
    assert_eq!(
        emulate_response.result,
        code.into_iter().skip(32).collect::<Vec<_>>()
    );
    assert_eq!(emulate_response.steps_executed, 17);
    assert_eq!(emulate_response.used_gas, 25_000);
    assert_eq!(emulate_response.iterations, 3);
    assert_eq!(emulate_response.solana_accounts.len(), 3);

    let result = into_traces(tracer, emulate_response);

    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        expected_trace.to_string()
    );
}
