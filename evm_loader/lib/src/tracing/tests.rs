use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use async_trait::async_trait;
use ethnum::U256;
use evm_loader::account::ether_balance::Header;
use evm_loader::account::{
    set_tag, BalanceAccount, ContractAccount, TAG_ACCOUNT_BALANCE, TAG_ACCOUNT_CONTRACT,
};
use map_macro::hash_map;
use solana_client::client_error::ClientError;
use solana_client::client_error::ClientErrorKind;
use solana_client::client_error::Result as ClientResult;
use solana_client::rpc_config::{RpcSendTransactionConfig, RpcTransactionConfig};
use solana_client::rpc_response::{
    Response, RpcResponseContext, RpcResult, RpcSimulateTransactionResult,
};
use solana_sdk::account::Account;
use solana_sdk::account_info::AccountInfo;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::rent::Rent;
use solana_sdk::signature::Signature;
use solana_transaction_status::{
    EncodedConfirmedBlock, EncodedConfirmedTransactionWithStatusMeta, TransactionStatus,
};

use crate::account_storage::EmulatorAccountStorage;
use crate::commands::emulate::{emulate_trx, EmulateResponse};
use crate::commands::get_config::ChainInfo;
use crate::commands::trace::{into_traces, to_emulation_result};
use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::TracerType;
use evm_loader::executor::ExecutorState;
use evm_loader::solana_program::clock::{Slot, UnixTimestamp};
use evm_loader::solana_program::hash::Hash;
use evm_loader::solana_program::instruction::Instruction;
use evm_loader::solana_program::pubkey::Pubkey;
use evm_loader::types::Address;

use crate::rpc::{e, Rpc};
use crate::syscall_stubs::EmulatorStubs;
use crate::tracing::tracers::new_tracer;
use crate::tracing::tracers::openeth::types::to_call_analytics;
use crate::tracing::TraceConfig;
use crate::types::TxParams;

struct TestRpc {
    accounts: HashMap<Pubkey, Account>,
    rent: Rent,
}

#[async_trait(?Send)]
impl Rpc for TestRpc {
    fn commitment(&self) -> CommitmentConfig {
        todo!()
    }

    async fn confirm_transaction_with_spinner(
        &self,
        _signature: &Signature,
        _recent_blockhash: &Hash,
        _commitment_config: CommitmentConfig,
    ) -> ClientResult<()> {
        todo!()
    }

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
        todo!()
    }

    async fn get_multiple_accounts(
        &self,
        _pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        Ok(vec![])
    }

    async fn get_account_data(&self, key: &Pubkey) -> ClientResult<Vec<u8>> {
        bincode::serialize(&self.rent).map_err(|e| e!("load account error", key, e))
    }

    async fn get_block(&self, _slot: Slot) -> ClientResult<EncodedConfirmedBlock> {
        todo!()
    }

    async fn get_block_time(&self, _slot: Slot) -> ClientResult<UnixTimestamp> {
        Ok(1234)
    }

    async fn get_latest_blockhash(&self) -> ClientResult<Hash> {
        todo!()
    }

    async fn get_minimum_balance_for_rent_exemption(&self, _data_len: usize) -> ClientResult<u64> {
        todo!()
    }

    async fn get_slot(&self) -> ClientResult<Slot> {
        Ok(123)
    }

    async fn get_signature_statuses(
        &self,
        _signatures: &[Signature],
    ) -> RpcResult<Vec<Option<TransactionStatus>>> {
        todo!()
    }

    async fn get_transaction_with_config(
        &self,
        _signature: &Signature,
        _config: RpcTransactionConfig,
    ) -> ClientResult<EncodedConfirmedTransactionWithStatusMeta> {
        todo!()
    }

    async fn send_transaction(
        &self,
        _transaction: &solana_sdk::transaction::Transaction,
    ) -> ClientResult<Signature> {
        todo!()
    }

    async fn send_and_confirm_transaction_with_spinner(
        &self,
        _transaction: &solana_sdk::transaction::Transaction,
    ) -> ClientResult<Signature> {
        todo!()
    }

    async fn send_and_confirm_transaction_with_spinner_and_commitment(
        &self,
        _transaction: &solana_sdk::transaction::Transaction,
        _commitment: CommitmentConfig,
    ) -> ClientResult<Signature> {
        todo!()
    }

    async fn send_and_confirm_transaction_with_spinner_and_config(
        &self,
        _transaction: &solana_sdk::transaction::Transaction,
        _commitment: CommitmentConfig,
        _config: RpcSendTransactionConfig,
    ) -> ClientResult<Signature> {
        todo!()
    }

    async fn get_latest_blockhash_with_commitment(
        &self,
        _commitment: CommitmentConfig,
    ) -> ClientResult<(Hash, u64)> {
        todo!()
    }

    fn can_simulate_transaction(&self) -> bool {
        todo!()
    }

    async fn simulate_transaction(
        &self,
        _signer: Option<Pubkey>,
        _instructions: &[Instruction],
    ) -> RpcResult<RpcSimulateTransactionResult> {
        todo!()
    }

    async fn get_account_with_sol(&self) -> ClientResult<Pubkey> {
        todo!()
    }

    fn as_any(&self) -> &dyn Any {
        todo!()
    }
}

#[tokio::test]
// tx_hash: 0xe888205d8f3f504d45da558ea0aea42a35130dad77f5e34c940acce3ca9182a8
async fn test_trace_contract_creation() {
    let gas_used = Some(U256::from(19010800u64));
    let chain_id = 1234;

    let trace_config = TraceConfig::default();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let (_code, emulation_response) = trace_contract_creation(gas_used, &tracer, chain_id).await;

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(
    //     emulation_result.exit_status,
    //     ExitStatus::Return(code.into_iter().skip(32).collect())
    // );
    assert_eq!(emulation_response.exit_status, "succeed");
    assert_eq!(emulation_response.steps_executed, 17);

    let result = into_traces(tracer, to_emulation_result(emulation_response));

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":19010800,\"failed\":false,\"returnValue\":\"608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":21,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\"]},{\"pc\":25,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\"]},{\"pc\":27,\"op\":\"CODECOPY\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\",\"0x0\"]},{\"pc\":28,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":30,\"op\":\"RETURN\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x0\"]}]}".to_string());
    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":19010800,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":21,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\"]},{\"pc\":25,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\"]},{\"pc\":27,\"op\":\"CODECOPY\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x1e3\",\"0x20\",\"0x0\"]},{\"pc\":28,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\"]},{\"pc\":30,\"op\":\"RETURN\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x1e3\",\"0x0\"]}]}".to_string());
}

#[tokio::test]
// tx_hash: 0xe888205d8f3f504d45da558ea0aea42a35130dad77f5e34c940acce3ca9182a8
async fn test_trace_state_diff_contract_creation() {
    let gas_used = Some(U256::from(19010800u64));
    let chain_id = 1234;

    let trace_config = state_diff_trace_config();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let (_code, emulate_response) = trace_contract_creation(gas_used, &tracer, chain_id).await;

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(
    //     emulation_result.exit_status,
    //     ExitStatus::Return(code.into_iter().skip(32).collect())
    // );
    assert_eq!(emulate_response.exit_status, "succeed");
    assert_eq!(emulate_response.steps_executed, 17);

    let result = into_traces(tracer, to_emulation_result(emulate_response));

    // TODO: Fix in NDEV-2329: https://github.com/neonlabsorg/neon-evm/pull/227
    // assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":{\"+\":\"0x0\"},\"nonce\":{\"+\":\"0x1\"},\"code\":{\"+\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\"},\"storage\":{}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4c8447cf1ec3d2090\",\"to\":\"0x4693551a823c1c310\"}},\"nonce\":{\"*\":{\"from\":\"0x5\",\"to\":\"0x6\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());
    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":{\"+\":\"0x0\"},\"nonce\":{\"+\":\"0x1\"},\"code\":{\"+\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\"},\"storage\":{}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4c8447cf1ec3d2090\",\"to\":\"0x4693551a823c1c310\"}},\"nonce\":{\"*\":{\"from\":\"0x5\",\"to\":\"0x6\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());
}

pub fn writable_account_info<'a>(key: &'a Pubkey, account: &'a mut Account) -> AccountInfo<'a> {
    AccountInfo {
        key,
        is_signer: false,
        is_writable: true,
        lamports: Rc::new(RefCell::new(&mut account.lamports)),
        data: Rc::new(RefCell::new(&mut account.data)),
        owner: &account.owner,
        executable: account.executable,
        rent_epoch: account.rent_epoch,
    }
}

async fn trace_contract_creation(
    gas_used: Option<U256>,
    tracer: &TracerType,
    chain_id: u64,
) -> (Vec<u8>, EmulateResponse) {
    let program_id = Pubkey::new_unique();

    let rent = set_up_rent_sysvar();

    let from = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();

    let (from_pubkey, from_account) = account_with_data(
        &program_id,
        from,
        Header {
            chain_id,
            balance: U256::from_str_hex("0x4c8447cf1ec3d2090").unwrap(),
            trx_count: 5,
        },
        chain_id,
    );

    let rpc = TestRpc {
        accounts: hash_map! {
            from_pubkey => from_account
        },
        rent,
    };

    let mut test_account_storage = test_emulator_account_storage(program_id, &rpc, chain_id).await;

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

    let emulation_result = emulate_trx(tx_params, &mut backend, 1000, Some(Rc::clone(tracer)))
        .await
        .unwrap();

    (code, emulation_result)
}

fn account_with_data(
    program_id: &Pubkey,
    address: Address,
    data: Header,
    chain_id: u64,
) -> (Pubkey, Account) {
    let mut account = Account::new(0, 10000, program_id);
    let (pubkey, _) = address.find_balance_address(program_id, chain_id);

    let account_info = writable_account_info(&pubkey, &mut account);

    set_tag(program_id, &account_info, TAG_ACCOUNT_BALANCE).unwrap();
    let mut balance_account = BalanceAccount {
        address: Some(address),
        account: account_info,
    };
    *balance_account.header_mut() = data;

    (pubkey, account)
}

#[tokio::test]
// tx_hash: 0xf1a8130526ff5951a8d1c7e31623f23ec84d8644514e5513a440e139a30f5166
async fn test_trace_increment_call() {
    let gas_used = Some(U256::from(10_000u64));
    let chain_id = 1234;

    let trace_config = TraceConfig::default();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let origin = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();
    let target = Address::from_hex("0x356726f027a805fab3bd7dd0413a96d81bc6f599").unwrap();

    let index = U256::ZERO;

    let program_id = Pubkey::new_unique();

    let rpc = increment_call_test_rpc(program_id, origin, target, index, chain_id).await;

    let mut test_account_storage = test_emulator_account_storage(program_id, &rpc, chain_id).await;

    let trx = increment_tx_params(gas_used, origin, target).await;

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let emulate_response = emulate_trx(trx, &mut backend, 1000, Some(Rc::clone(&tracer)))
        .await
        .unwrap();

    assert_eq!(emulate_response.exit_status, "succeed"); // TODO why stop?
    assert_eq!(emulate_response.steps_executed, 112);

    let result = into_traces(tracer, to_emulation_result(emulate_response));

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":10000,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":20,\"op\":\"CALLDATASIZE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x4\"]},{\"pc\":21,\"op\":\"LT\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x4\",\"0x4\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":25,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x41\"]},{\"pc\":26,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":28,\"op\":\"CALLDATALOAD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":29,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a00000000000000000000000000000000000000000000000000000000\"]},{\"pc\":31,\"op\":\"SHR\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a00000000000000000000000000000000000000000000000000000000\",\"0xe0\"]},{\"pc\":32,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":33,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":38,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0x2e64cec1\"]},{\"pc\":39,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\"]},{\"pc\":42,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\",\"0x46\"]},{\"pc\":43,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":44,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":49,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0x6057361d\"]},{\"pc\":50,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\"]},{\"pc\":53,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\",\"0x64\"]},{\"pc\":54,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":55,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":60,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":61,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x1\"]},{\"pc\":64,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x1\",\"0x80\"]},{\"pc\":128,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":129,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":132,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":135,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x9d\"]},{\"pc\":157,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":158,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":160,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\"]},{\"pc\":162,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\"]},{\"pc\":163,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\"]},{\"pc\":164,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\"]},{\"pc\":165,\"op\":\"SLOAD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0x0\"],\"storage\":{\"0000000000000000000000000000000000000000000000000000000000000000\":\"000000000000000000000000000000000000000000000000000000000000000f\"}},{\"pc\":166,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0xf\"]},{\"pc\":169,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0xf\",\"0xaf\"]},{\"pc\":170,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0xf\",\"0x1\"]},{\"pc\":171,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":174,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x179\"]},{\"pc\":377,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":378,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":380,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":383,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\"]},{\"pc\":384,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":387,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xb8\"]},{\"pc\":184,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":185,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":187,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0x0\"]},{\"pc\":188,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":189,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xf\",\"0x0\"]},{\"pc\":190,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xf\"]},{\"pc\":191,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0xf\",\"0x184\"]},{\"pc\":192,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0x184\",\"0xf\"]},{\"pc\":193,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0x184\"]},{\"pc\":388,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":389,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":390,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":391,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":394,\"op\":\"DUP4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\"]},{\"pc\":395,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":398,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0xb8\"]},{\"pc\":184,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":185,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":187,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x0\"]},{\"pc\":188,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x0\",\"0x1\"]},{\"pc\":189,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x1\",\"0x0\"]},{\"pc\":190,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x1\"]},{\"pc\":191,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x1\",\"0x18f\"]},{\"pc\":192,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x18f\",\"0x1\"]},{\"pc\":193,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x18f\"]},{\"pc\":399,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":400,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":401,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":402,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":403,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":404,\"op\":\"ADD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0xf\"]},{\"pc\":405,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x10\"]},{\"pc\":406,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x0\"]},{\"pc\":407,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":408,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x10\"]},{\"pc\":409,\"op\":\"GT\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x10\",\"0xf\"]},{\"pc\":410,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x0\"]},{\"pc\":411,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x1\"]},{\"pc\":414,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x1\",\"0x1a7\"]},{\"pc\":423,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":424,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":425,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0x1\",\"0xf\",\"0xaf\"]},{\"pc\":426,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\",\"0xf\",\"0x1\"]},{\"pc\":427,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\",\"0xf\"]},{\"pc\":428,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\"]},{\"pc\":175,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\"]},{\"pc\":176,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\"]},{\"pc\":177,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x0\",\"0x1\"]},{\"pc\":178,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x0\"]},{\"pc\":179,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\"]},{\"pc\":180,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x10\"]},{\"pc\":181,\"op\":\"SSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x10\",\"0x0\"],\"storage\":{\"0000000000000000000000000000000000000000000000000000000000000000\":\"0000000000000000000000000000000000000000000000000000000000000010\"}},{\"pc\":182,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\"]},{\"pc\":183,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":136,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":137,\"op\":\"STOP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]}]}".to_string());

    assert_eq!(
        U256::from_be_bytes(backend.storage(target, index).await.unwrap()),
        U256::from_be_bytes(test_account_storage.storage(target, index).await) + 1
    );
}

async fn test_emulator_account_storage(
    program_id: Pubkey,
    rpc: &impl Rpc,
    chain_id: u64,
) -> EmulatorAccountStorage {
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

#[tokio::test]
// tx_hash: 0xf1a8130526ff5951a8d1c7e31623f23ec84d8644514e5513a440e139a30f5166
async fn test_trace_state_diff_increment_call() {
    let gas_used = Some(U256::from(10_000u64));
    let chain_id = 1234;

    let trace_config = state_diff_trace_config();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let origin = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();
    let target = Address::from_hex("0x356726f027a805fab3bd7dd0413a96d81bc6f599").unwrap();

    let index = U256::ZERO;

    let program_id = Pubkey::new_unique();

    let rpc = increment_call_test_rpc(program_id, origin, target, index, chain_id).await;

    let mut test_account_storage = test_emulator_account_storage(program_id, &rpc, chain_id).await;

    let trx = increment_tx_params(gas_used, origin, target).await;

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let emulate_response = emulate_trx(trx, &mut backend, 1000, Some(Rc::clone(&tracer)))
        .await
        .unwrap();

    assert_eq!(emulate_response.exit_status, "succeed"); // todo why stop?
    assert_eq!(emulate_response.steps_executed, 112);

    let result = into_traces(tracer, to_emulation_result(emulate_response));

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":\"=\",\"nonce\":\"=\",\"code\":\"=\",\"storage\":{\"0x0000000000000000000000000000000000000000000000000000000000000000\":{\"*\":{\"from\":\"0x000000000000000000000000000000000000000000000000000000000000000f\",\"to\":\"0x0000000000000000000000000000000000000000000000000000000000000010\"}}}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4692885e28cc02260\",\"to\":\"0x4691bba948aba7cc0\"}},\"nonce\":{\"*\":{\"from\":\"0x7\",\"to\":\"0x8\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());

    assert_eq!(
        U256::from_be_bytes(backend.storage(target, index).await.unwrap()),
        U256::from_be_bytes(test_account_storage.storage(target, index).await) + 1
    );
}

async fn increment_tx_params(gas_used: Option<U256>, origin: Address, target: Address) -> TxParams {
    TxParams {
        from: origin,
        to: Some(target),
        data: Some(hex::decode("d09de08a").unwrap()),
        gas_used,
        gas_price: Some(U256::from(360_123_562_234_u64)),
        gas_limit: Some(U256::from(30_000u64)),
        ..TxParams::default()
    }
}

async fn increment_call_test_rpc(
    program_id: Pubkey,
    origin: Address,
    target: Address,
    index: U256,
    chain_id: u64,
) -> impl Rpc {
    let rent = set_up_rent_sysvar();

    let (origin_pubkey, origin_account) = account_with_data(
        &program_id,
        origin,
        Header {
            balance: U256::from_str_hex("0x4692885e28cc02260").unwrap(),
            trx_count: 7,
            chain_id,
        },
        chain_id,
    );

    let (target_pubkey, _) = target.find_balance_address(&program_id, chain_id);
    let mut target_account = Account::new(0, 10000, &program_id);
    {
        let account_info = writable_account_info(&target_pubkey, &mut target_account);

        set_tag(&program_id, &account_info, TAG_ACCOUNT_BALANCE).unwrap();
        let mut balance_account = BalanceAccount {
            address: Some(target),
            account: account_info,
        };
        *balance_account.header_mut() = Header {
            balance: Default::default(),
            trx_count: 1,
            chain_id,
        };
    }

    let code_vec = hex::decode("608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033").unwrap();

    let (target_code_pubkey, _) = target.find_solana_address(&program_id);
    let mut target_code_account = Account::new(
        0,
        ContractAccount::required_account_size(&code_vec),
        &program_id,
    );
    let code_account_info = writable_account_info(&target_code_pubkey, &mut target_code_account);

    set_tag(&program_id, &code_account_info, TAG_ACCOUNT_CONTRACT).unwrap();
    let mut contract = ContractAccount::from_account(&program_id, code_account_info).unwrap();
    {
        let mut header = contract.header_mut();
        header.chain_id = chain_id;
        header.generation = 0;
    }
    {
        let mut contract_code = contract.code_mut();
        contract_code.copy_from_slice(&code_vec);
    }
    contract.set_storage_value(
        index.as_usize(),
        &U256::from_str_hex("0x0f").unwrap().to_be_bytes(),
    );

    TestRpc {
        accounts: hash_map! {
            origin_pubkey => origin_account,
            target_pubkey => target_account,
            target_code_pubkey => target_code_account
        },
        rent,
    }
}

fn set_up_rent_sysvar() -> Rent {
    let rent = Rent {
        lamports_per_byte_year: 0,
        exemption_threshold: 0.0,
        burn_percent: 0,
    };
    solana_sdk::program_stubs::set_syscall_stubs(Box::new(EmulatorStubs::new_rent(rent)));
    rent
}

#[tokio::test]
// tx_hash: 0xa3c0a2d8f7519217775ca49e836cdfffec8cd1d16950553f3b41b580d10d44b7
async fn test_trace_transfer_transaction() {
    let origin = Address::from_hex("0x4bbac480d466865807ec5e98ffdf429c170e2a4e").unwrap();
    let target = Address::from_hex("0xf2418612ef70c2207da5d42511b7f58587ba27e3").unwrap();
    let chain_id = 1234;

    let program_id = Pubkey::new_unique();

    let rpc = transfer_transaction_rpc(program_id, origin, target, chain_id);

    let mut test_account_storage = test_emulator_account_storage(program_id, &rpc, chain_id).await;

    let value = U256::from(100000000000000000u64);

    let gas_used = Some(U256::from(10_000u64));

    let trx = transfer_tx_params(origin, target, value, gas_used).await;

    let trace_config = TraceConfig::default();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let emulate_response = emulate_trx(trx, &mut backend, 100, Some(Rc::clone(&tracer)))
        .await
        .unwrap();

    assert_eq!(emulate_response.exit_status, "succeed");
    assert_eq!(emulate_response.steps_executed, 1);

    let result = into_traces(tracer, to_emulation_result(emulate_response));

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"gas\":10000,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"STOP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]}]}".to_string());

    assert_eq!(
        backend.balance(target, chain_id).await.unwrap()
            - test_account_storage
                .balance(target, chain_id)
                .await
                .unwrap(),
        value
    );
}

#[tokio::test]
// tx_hash: 0xa3c0a2d8f7519217775ca49e836cdfffec8cd1d16950553f3b41b580d10d44b7
async fn test_trace_state_diff_transfer_transaction() {
    let origin = Address::from_hex("0x4bbac480d466865807ec5e98ffdf429c170e2a4e").unwrap();
    let target = Address::from_hex("0xf2418612ef70c2207da5d42511b7f58587ba27e3").unwrap();
    let chain_id = 1234;

    let program_id = Pubkey::new_unique();

    let rpc = transfer_transaction_rpc(program_id, origin, target, chain_id);

    let mut test_account_storage = test_emulator_account_storage(program_id, &rpc, chain_id).await;

    let value = U256::from(100_000_000_000_000_000u64);

    let gas_used = Some(U256::from(10_000u64));

    let trx = transfer_tx_params(origin, target, value, gas_used).await;

    let trace_config = state_diff_trace_config();
    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let emulate_response = emulate_trx(trx, &mut backend, 1000, Some(Rc::clone(&tracer)))
        .await
        .unwrap();

    assert_eq!(emulate_response.exit_status, "succeed");
    assert_eq!(emulate_response.steps_executed, 1);

    let result = into_traces(tracer, to_emulation_result(emulate_response));

    assert_eq!(serde_json::to_string(&result).unwrap(), "{\"output\":\"0x\",\"stateDiff\":{\"0x4bbac480d466865807ec5e98ffdf429c170e2a4e\":{\"balance\":{\"*\":{\"from\":\"0x6c6a20ec9d08c1b590\",\"to\":\"0x6c68ae7dae54436b20\"}},\"nonce\":{\"*\":{\"from\":\"0x1\",\"to\":\"0x2\"}},\"code\":\"=\",\"storage\":{}},\"0xf2418612ef70c2207da5d42511b7f58587ba27e3\":{\"balance\":{\"*\":{\"from\":\"0x6c6cf6a1041aca0000\",\"to\":\"0x6c6e59e67c78540000\"}},\"nonce\":\"=\",\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}".to_string());

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
) -> impl Rpc {
    let rent = set_up_rent_sysvar();

    let (origin_pubkey, origin_account) = account_with_data(
        &program_id,
        origin,
        Header {
            chain_id,
            trx_count: 1,
            balance: U256::from_str_hex("0x6c6a20ec9d08c1b590").unwrap(),
        },
        chain_id,
    );

    let (target_pubkey, target_account) = account_with_data(
        &program_id,
        target,
        Header {
            chain_id,
            trx_count: 0,
            balance: U256::from_str_hex("0x6c6cf6a1041aca0000").unwrap(),
        },
        chain_id,
    );

    TestRpc {
        accounts: hash_map! {
            origin_pubkey => origin_account,
            target_pubkey => target_account
        },
        rent,
    }
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

fn state_diff_trace_config() -> TraceConfig {
    TraceConfig {
        tracer: Some("openethereum".to_string()),
        tracer_config: Some(
            serde_json::to_value(to_call_analytics(&vec!["stateDiff".to_string()])).unwrap(),
        ),
        ..TraceConfig::default()
    }
}
