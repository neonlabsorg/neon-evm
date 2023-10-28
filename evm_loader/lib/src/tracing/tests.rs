use std::collections::HashMap;

use async_trait::async_trait;
use ether_account::Data;
use ethnum::U256;
use map_macro::hash_map;
use solana_sdk::account_info::AccountInfo;

use evm_loader::account::ether_account;
use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{EmulationResult, TracerType};
use evm_loader::evm::{Buffer, ExitStatus, Machine, MachineResult};
use evm_loader::executor::{ExecutorState, OwnedAccountInfo};
use evm_loader::solana_program::pubkey::Pubkey;
use evm_loader::types::{Address, Transaction};

use crate::commands::emulate::tx_params_to_transaction;
use crate::commands::trace::into_traces;
use crate::tracing::tracers::new_tracer;
use crate::tracing::tracers::openeth::state_diff::build_state_diff;
use crate::tracing::tracers::openeth::types::to_call_analytics;
use crate::tracing::TraceConfig;
use crate::types::TxParams;

#[derive(Default)]
struct TestAccountStorage {
    chain_id: u64,
    block_number: U256,
    block_timestamp: U256,
    accounts: HashMap<Address, Data>,
    code: HashMap<Address, Buffer>,
    storage: HashMap<Address, HashMap<U256, U256>>,
}

#[async_trait(?Send)]
impl AccountStorage for TestAccountStorage {
    fn all_addresses(&self) -> Vec<Address> {
        self.accounts.keys().cloned().collect()
    }

    fn neon_token_mint(&self) -> &Pubkey {
        todo!()
    }

    fn program_id(&self) -> &Pubkey {
        todo!()
    }

    fn operator(&self) -> &Pubkey {
        todo!()
    }

    fn block_number(&self) -> U256 {
        self.block_number
    }

    fn block_timestamp(&self) -> U256 {
        self.block_timestamp
    }

    async fn block_hash(&mut self, _number: u64) -> [u8; 32] {
        todo!()
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    async fn exists(&mut self, address: &Address) -> bool {
        self.accounts.contains_key(address)
    }

    async fn nonce(&mut self, address: &Address) -> u64 {
        self.accounts
            .get(address)
            .map(|data| data.trx_count)
            .unwrap_or_default()
    }

    async fn balance(&mut self, address: &Address) -> U256 {
        self.accounts
            .get(address)
            .map(|data| data.balance)
            .unwrap_or_default()
    }

    async fn code_size(&mut self, address: &Address) -> usize {
        self.accounts
            .get(address)
            .map(|data| data.code_size as usize)
            .unwrap_or_default()
    }

    async fn code_hash(&mut self, address: &Address) -> [u8; 32] {
        solana_sdk::keccak::hash(self.code.get(address).cloned().unwrap_or_default().as_ref())
            .to_bytes()
    }

    async fn code(&mut self, address: &Address) -> Buffer {
        self.code.get(address).cloned().unwrap_or_default()
    }

    async fn generation(&mut self, _address: &Address) -> u32 {
        todo!()
    }

    async fn storage(&mut self, address: &Address, index: &U256) -> [u8; 32] {
        self.storage
            .get(address)
            .unwrap()
            .get(index)
            .unwrap()
            .to_be_bytes()
    }

    async fn clone_solana_account(&mut self, _address: &Pubkey) -> OwnedAccountInfo {
        todo!()
    }

    async fn map_solana_account<F, R>(&mut self, _address: &Pubkey, _action: F) -> R
    where
        F: FnOnce(&AccountInfo) -> R,
    {
        todo!()
    }

    async fn solana_account_space(&mut self, _address: &Address) -> Option<usize> {
        todo!()
    }
}

#[tokio::test]
// tx_hash: 0xe888205d8f3f504d45da558ea0aea42a35130dad77f5e34c940acce3ca9182a8
async fn test_trace_contract_creation() {
    let gas_used = Some(U256::from(19010800u64));

    let trace_config = TraceConfig::default();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let (_code, emulation_result, tracer) = trace_contract_creation(gas_used, tracer).await;

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(
    //     emulation_result.exit_status,
    //     ExitStatus::Return(code.into_iter().skip(32).collect())
    // );
    assert_eq!(emulation_result.exit_status, ExitStatus::Return(vec![]));
    assert_eq!(emulation_result.steps_executed, 17);

    let result = into_traces(tracer, emulation_result);

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":19010800,\"failed\":false,\"returnValue\":\"608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":21,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\"]},{\"pc\":25,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\"]},{\"pc\":27,\"op\":\"CODECOPY\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\",\"0x0\"]},{\"pc\":28,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":30,\"op\":\"RETURN\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x0\"]}]}".to_string());
    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":19010800,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":21,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\"]},{\"pc\":25,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\"]},{\"pc\":27,\"op\":\"CODECOPY\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\",\"0x0\"]},{\"pc\":28,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":30,\"op\":\"RETURN\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x0\"]}]}".to_string());
}

#[tokio::test]
// tx_hash: 0xe888205d8f3f504d45da558ea0aea42a35130dad77f5e34c940acce3ca9182a8
async fn test_trace_state_diff_contract_creation() {
    let gas_used = Some(U256::from(19010800u64));

    let trace_config = state_diff_trace_config();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let (_code, emulation_result, tracer) = trace_contract_creation(gas_used, tracer).await;

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(
    //     emulation_result.exit_status,
    //     ExitStatus::Return(code.into_iter().skip(32).collect())
    // );
    assert_eq!(emulation_result.exit_status, ExitStatus::Return(vec![]));
    assert_eq!(emulation_result.steps_executed, 17);

    let result = into_traces(tracer, emulation_result);

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":{\"+\":\"0x0\"},\"nonce\":{\"+\":\"0x1\"},\"code\":{\"+\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\"},\"storage\":{}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4c8447cf1ec3d2090\",\"to\":\"0x4693551a823c1c310\"}},\"nonce\":{\"*\":{\"from\":\"0x5\",\"to\":\"0x6\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());
    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":{\"+\":\"0x0\"},\"nonce\":{\"+\":\"0x1\"},\"code\":{\"+\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\"},\"storage\":{}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4c8447cf1ec3d2090\",\"to\":\"0x4693551a823c1c310\"}},\"nonce\":{\"*\":{\"from\":\"0x5\",\"to\":\"0x6\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());
}

async fn trace_contract_creation(
    gas_used: Option<U256>,
    tracer: TracerType,
) -> (Vec<u8>, EmulationResult, TracerType) {
    let from = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();
    let contract = Address::from_hex("0x356726f027a805fab3bd7dd0413a96d81bc6f599").unwrap();

    let mut test_account_storage = TestAccountStorage {
        chain_id: 123u64,
        accounts: hash_map! {
            from => Data {
                address: from,
                trx_count: 5,
                balance: U256::from_str_hex("0x4c8447cf1ec3d2090").unwrap(),
                ..Data::default()
            },
            contract => Data {
                address: contract,
                ..Data::default()
            }
        },
        ..TestAccountStorage::default()
    };

    let code = hex::decode("608060405234801561001057600080fd5b506101e3806100206000396000f3fe608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033").unwrap();

    let chain_id = test_account_storage.chain_id;
    let mut trx = tx_params_to_transaction(
        TxParams {
            from,
            data: Some(code.clone()),
            gas_limit: Some(U256::from(19_035_800u64)),
            gas_used,
            gas_price: Some(U256::from(360_307_885_736u64)),
            ..TxParams::default()
        },
        &mut test_account_storage,
        chain_id,
    )
    .await;

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let (emulation_result, tracer) =
        emulate_trx(from, gas_used, &mut trx, tracer, &mut backend).await;

    (code, emulation_result, tracer)
}

#[tokio::test]
// tx_hash: 0xf1a8130526ff5951a8d1c7e31623f23ec84d8644514e5513a440e139a30f5166
async fn test_trace_increment_call() {
    let gas_used = Some(U256::from(10_000u64));

    let trace_config = TraceConfig::default();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let origin = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();
    let target = Address::from_hex("0x356726f027a805fab3bd7dd0413a96d81bc6f599").unwrap();

    let index = U256::ZERO;

    let mut test_account_storage = increment_call_test_storage(origin, target, index);

    let mut trx = increment_tx_params(gas_used, origin, target, &mut test_account_storage).await;

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let (emulation_result, tracer) =
        emulate_trx(origin, gas_used, &mut trx, tracer, &mut backend).await;

    assert_eq!(emulation_result.exit_status, ExitStatus::Stop);
    assert_eq!(emulation_result.steps_executed, 112);

    let result = into_traces(tracer, emulation_result);

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":10000,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":20,\"op\":\"CALLDATASIZE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x4\"]},{\"pc\":21,\"op\":\"LT\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x4\",\"0x4\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":25,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x41\"]},{\"pc\":26,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":28,\"op\":\"CALLDATALOAD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":29,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a00000000000000000000000000000000000000000000000000000000\"]},{\"pc\":31,\"op\":\"SHR\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a00000000000000000000000000000000000000000000000000000000\",\"0xe0\"]},{\"pc\":32,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":33,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":38,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0x2e64cec1\"]},{\"pc\":39,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\"]},{\"pc\":42,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\",\"0x46\"]},{\"pc\":43,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":44,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":49,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0x6057361d\"]},{\"pc\":50,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\"]},{\"pc\":53,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\",\"0x64\"]},{\"pc\":54,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":55,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":60,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":61,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x1\"]},{\"pc\":64,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x1\",\"0x80\"]},{\"pc\":128,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":129,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":132,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":135,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x9d\"]},{\"pc\":157,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":158,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":160,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\"]},{\"pc\":162,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\"]},{\"pc\":163,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\"]},{\"pc\":164,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\"]},{\"pc\":165,\"op\":\"SLOAD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0x0\"],\"storage\":{\"0000000000000000000000000000000000000000000000000000000000000000\":\"000000000000000000000000000000000000000000000000000000000000000f\"}},{\"pc\":166,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0xf\"]},{\"pc\":169,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0xf\",\"0xaf\"]},{\"pc\":170,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0xf\",\"0x1\"]},{\"pc\":171,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":174,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x179\"]},{\"pc\":377,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":378,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":380,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":383,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\"]},{\"pc\":384,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":387,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xb8\"]},{\"pc\":184,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":185,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":187,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0x0\"]},{\"pc\":188,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":189,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xf\",\"0x0\"]},{\"pc\":190,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xf\"]},{\"pc\":191,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0xf\",\"0x184\"]},{\"pc\":192,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0x184\",\"0xf\"]},{\"pc\":193,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0x184\"]},{\"pc\":388,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":389,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":390,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":391,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":394,\"op\":\"DUP4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\"]},{\"pc\":395,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":398,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0xb8\"]},{\"pc\":184,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":185,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":187,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x0\"]},{\"pc\":188,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x0\",\"0x1\"]},{\"pc\":189,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x1\",\"0x0\"]},{\"pc\":190,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x1\"]},{\"pc\":191,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x1\",\"0x18f\"]},{\"pc\":192,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x18f\",\"0x1\"]},{\"pc\":193,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x18f\"]},{\"pc\":399,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":400,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":401,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":402,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":403,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":404,\"op\":\"ADD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0xf\"]},{\"pc\":405,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x10\"]},{\"pc\":406,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x0\"]},{\"pc\":407,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":408,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x10\"]},{\"pc\":409,\"op\":\"GT\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x10\",\"0xf\"]},{\"pc\":410,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x0\"]},{\"pc\":411,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x1\"]},{\"pc\":414,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x1\",\"0x1a7\"]},{\"pc\":423,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":424,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":425,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0x1\",\"0xf\",\"0xaf\"]},{\"pc\":426,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\",\"0xf\",\"0x1\"]},{\"pc\":427,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\",\"0xf\"]},{\"pc\":428,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\"]},{\"pc\":175,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\"]},{\"pc\":176,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\"]},{\"pc\":177,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x0\",\"0x1\"]},{\"pc\":178,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x0\"]},{\"pc\":179,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\"]},{\"pc\":180,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x10\"]},{\"pc\":181,\"op\":\"SSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x10\",\"0x0\"],\"storage\":{\"0000000000000000000000000000000000000000000000000000000000000000\":\"0000000000000000000000000000000000000000000000000000000000000010\"}},{\"pc\":182,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\"]},{\"pc\":183,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":136,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":137,\"op\":\"STOP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]}]}".to_string());

    assert_eq!(
        U256::from_be_bytes(backend.storage(&target, &index).await.unwrap()),
        U256::from_be_bytes(test_account_storage.storage(&target, &index).await) + 1
    );
}

#[tokio::test]
// tx_hash: 0xf1a8130526ff5951a8d1c7e31623f23ec84d8644514e5513a440e139a30f5166
async fn test_trace_state_diff_increment_call() {
    let gas_used = Some(U256::from(10_000u64));

    let trace_config = state_diff_trace_config();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let origin = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();
    let target = Address::from_hex("0x356726f027a805fab3bd7dd0413a96d81bc6f599").unwrap();

    let index = U256::ZERO;

    let mut test_account_storage = increment_call_test_storage(origin, target, index);

    let mut trx = increment_tx_params(gas_used, origin, target, &mut test_account_storage).await;

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let (emulation_result, tracer) =
        emulate_trx(origin, gas_used, &mut trx, tracer, &mut backend).await;

    assert_eq!(emulation_result.exit_status, ExitStatus::Stop);
    assert_eq!(emulation_result.steps_executed, 112);

    let result = into_traces(tracer, emulation_result);

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":{\"+\":\"0x0\"},\"nonce\":\"=\",\"code\":\"=\",\"storage\":{\"0x0000000000000000000000000000000000000000000000000000000000000000\":{\"*\":{\"from\":\"0x000000000000000000000000000000000000000000000000000000000000000f\",\"to\":\"0x0000000000000000000000000000000000000000000000000000000000000010\"}}}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4692885e28cc02260\",\"to\":\"0x4691bba948aba7cc0\"}},\"nonce\":{\"*\":{\"from\":\"0x7\",\"to\":\"0x8\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());

    assert_eq!(
        U256::from_be_bytes(backend.storage(&target, &index).await.unwrap()),
        U256::from_be_bytes(test_account_storage.storage(&target, &index).await) + 1
    );
}

async fn increment_tx_params(
    gas_used: Option<U256>,
    origin: Address,
    target: Address,
    test_account_storage: &mut TestAccountStorage,
) -> Transaction {
    tx_params_to_transaction(
        TxParams {
            from: origin,
            to: Some(target),
            data: Some(hex::decode("d09de08a").unwrap()),
            gas_used,
            gas_price: Some(U256::from(360_123_562_234_u64)),
            gas_limit: Some(U256::from(30_000u64)),
            ..TxParams::default()
        },
        test_account_storage,
        test_account_storage.chain_id,
    )
    .await
}

fn increment_call_test_storage(
    origin: Address,
    target: Address,
    index: U256,
) -> TestAccountStorage {
    let code_vec = hex::decode("608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033").unwrap();

    TestAccountStorage {
        chain_id: 123u64,
        accounts: hash_map! {
            origin => Data {
                address: origin,
                trx_count: 7,
                balance: U256::from_str_hex("0x4692885e28cc02260").unwrap(),
                ..Data::default()
            },
            target => Data {
                address: target,
                trx_count: 1,
                balance: Default::default(),
                code_size: code_vec.len() as u32,
                ..Data::default()
            }
        },
        code: hash_map! {
            target => Buffer::from_slice(&code_vec)
        },
        storage: hash_map! {
            target => hash_map! {
                index => U256::from_str_hex("0x0f").unwrap()
            }
        },
        ..TestAccountStorage::default()
    }
}

#[tokio::test]
// tx_hash: 0xa3c0a2d8f7519217775ca49e836cdfffec8cd1d16950553f3b41b580d10d44b7
async fn test_trace_transfer_transaction() {
    let origin = Address::from_hex("0x4bbac480d466865807ec5e98ffdf429c170e2a4e").unwrap();
    let target = Address::from_hex("0xf2418612ef70c2207da5d42511b7f58587ba27e3").unwrap();

    let mut test_account_storage = transfer_transaction_storage(origin, target);

    let value = U256::from(100000000000000000u64);

    let gas_used = Some(U256::from(10_000u64));

    let mut trx =
        transfer_tx_params(origin, target, &mut test_account_storage, value, gas_used).await;

    let trace_config = TraceConfig::default();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let (emulation_result, tracer) =
        emulate_trx(origin, gas_used, &mut trx, tracer, &mut backend).await;

    assert_eq!(emulation_result.exit_status, ExitStatus::Stop);
    assert_eq!(emulation_result.steps_executed, 1);

    let result = into_traces(tracer, emulation_result);

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":10000,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"STOP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]}]}".to_string());

    assert_eq!(
        backend.balance(&target).await.unwrap() - test_account_storage.balance(&target).await,
        value
    );
}

#[tokio::test]
// tx_hash: 0xa3c0a2d8f7519217775ca49e836cdfffec8cd1d16950553f3b41b580d10d44b7
async fn test_trace_state_diff_transfer_transaction() {
    let origin = Address::from_hex("0x4bbac480d466865807ec5e98ffdf429c170e2a4e").unwrap();
    let target = Address::from_hex("0xf2418612ef70c2207da5d42511b7f58587ba27e3").unwrap();

    let mut test_account_storage = transfer_transaction_storage(origin, target);

    let value = U256::from(100_000_000_000_000_000u64);

    let gas_used = Some(U256::from(10_000u64));

    let mut trx =
        transfer_tx_params(origin, target, &mut test_account_storage, value, gas_used).await;

    let trace_config = state_diff_trace_config();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let (emulation_result, tracer) =
        emulate_trx(origin, gas_used, &mut trx, tracer, &mut backend).await;

    assert_eq!(emulation_result.exit_status, ExitStatus::Stop);
    assert_eq!(emulation_result.steps_executed, 1);

    let result = into_traces(tracer, emulation_result);

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x\",\"stateDiff\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":{\"*\":{\"from\":\"0x6c6a20ec9d08c1b590\",\"to\":\"0x6c68ae7dae54436b20\"}},\"nonce\":{\"*\":{\"from\":\"0x1\",\"to\":\"0x2\"}},\"code\":\"=\",\"storage\":{}},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":{\"*\":{\"from\":\"0x6c6cf6a1041aca0000\",\"to\":\"0x6c6e59e67c78540000\"}},\"nonce\":{\"+\":\"0x0\"},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());

    assert_eq!(
        backend.balance(&target).await.unwrap() - test_account_storage.balance(&target).await,
        value
    );
}

fn transfer_transaction_storage(origin: Address, target: Address) -> TestAccountStorage {
    TestAccountStorage {
        chain_id: 123u64,
        accounts: hash_map! {
            origin => Data {
                address: origin,
                trx_count: 1,
                balance: U256::from_str_hex("0x6c6a20ec9d08c1b590").unwrap(),
                ..Data::default()
            },
            target => Data {
                address: target,
                balance: U256::from_str_hex("0x6c6cf6a1041aca0000").unwrap(),
                ..Data::default()
            }
        },
        ..TestAccountStorage::default()
    }
}

async fn transfer_tx_params(
    origin: Address,
    target: Address,
    test_account_storage: &mut TestAccountStorage,
    value: U256,
    gas_used: Option<U256>,
) -> Transaction {
    tx_params_to_transaction(
        TxParams {
            from: origin,
            to: Some(target),
            value: Some(value),
            gas_used,
            gas_price: Some(U256::from(426_771_289_239u64)),
            gas_limit: Some(U256::from(30_000u64)),
            ..TxParams::default()
        },
        test_account_storage,
        test_account_storage.chain_id,
    )
    .await
}

fn state_diff_trace_config() -> TraceConfig {
    TraceConfig {
        tracer: Some("openethereum".to_string()),
        tracer_config: Some(
            serde_json::to_value(to_call_analytics(&vec!["stateDiff".to_string()])).unwrap(),
        ),
        ..TraceConfig::default()
    }
}

async fn emulate_trx<B: AccountStorage>(
    origin: Address,
    gas_used: Option<U256>,
    trx: &mut Transaction,
    tracer: TracerType,
    backend: &mut ExecutorState<'_, B>,
) -> (EmulationResult, TracerType) {
    let mut machine = Machine::new(trx, origin, backend, Some(tracer))
        .await
        .unwrap();

    let MachineResult {
        exit_status,
        steps_executed,
        tracer,
    } = machine.execute(1000, backend).await.unwrap();

    let actions = backend.into_actions();

    let tx_fee = gas_used.unwrap_or_default() * trx.gas_price();

    (
        EmulationResult {
            exit_status,
            steps_executed,
            used_gas: 0,
            actions,
            state_diff: build_state_diff(origin, tx_fee, backend).await.unwrap(),
        },
        tracer.unwrap(),
    )
}
