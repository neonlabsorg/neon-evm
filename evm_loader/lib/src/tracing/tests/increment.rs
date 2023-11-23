use std::rc::Rc;

use ethnum::U256;
use map_macro::hash_map;
use solana_sdk::account::Account;

use evm_loader::account::ContractAccount;
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
async fn test_trace_increment_call() {
    trace_increment_call(TraceConfig::default(), "{\"gas\":10000,\"failed\":false,\"returnValue\":\"\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":2,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\"]},{\"pc\":4,\"op\":\"MSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x80\",\"0x40\"]},{\"pc\":5,\"op\":\"CALLVALUE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":6,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":7,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x0\"]},{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"]},{\"pc\":11,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\",\"0x10\"]},{\"pc\":16,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":17,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":18,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":20,\"op\":\"CALLDATASIZE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x4\"]},{\"pc\":21,\"op\":\"LT\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x4\",\"0x4\"]},{\"pc\":22,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":25,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x41\"]},{\"pc\":26,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[]},{\"pc\":28,\"op\":\"CALLDATALOAD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\"]},{\"pc\":29,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a00000000000000000000000000000000000000000000000000000000\"]},{\"pc\":31,\"op\":\"SHR\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a00000000000000000000000000000000000000000000000000000000\",\"0xe0\"]},{\"pc\":32,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":33,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":38,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0x2e64cec1\"]},{\"pc\":39,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\"]},{\"pc\":42,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\",\"0x46\"]},{\"pc\":43,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":44,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":49,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0x6057361d\"]},{\"pc\":50,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\"]},{\"pc\":53,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x0\",\"0x64\"]},{\"pc\":54,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":55,\"op\":\"PUSH4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":60,\"op\":\"EQ\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0xd09de08a\",\"0xd09de08a\"]},{\"pc\":61,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x1\"]},{\"pc\":64,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x1\",\"0x80\"]},{\"pc\":128,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":129,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":132,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":135,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x9d\"]},{\"pc\":157,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":158,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":160,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\"]},{\"pc\":162,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\"]},{\"pc\":163,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\"]},{\"pc\":164,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\"]},{\"pc\":165,\"op\":\"SLOAD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0x0\"],\"storage\":{\"0000000000000000000000000000000000000000000000000000000000000000\":\"000000000000000000000000000000000000000000000000000000000000000f\"}},{\"pc\":166,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0xf\"]},{\"pc\":169,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x1\",\"0xf\",\"0xaf\"]},{\"pc\":170,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0xf\",\"0x1\"]},{\"pc\":171,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":174,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x179\"]},{\"pc\":377,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":378,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\"]},{\"pc\":380,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":383,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\"]},{\"pc\":384,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":387,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xb8\"]},{\"pc\":184,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":185,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\"]},{\"pc\":187,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0x0\"]},{\"pc\":188,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":189,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xf\",\"0x0\"]},{\"pc\":190,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x184\",\"0xf\",\"0xf\"]},{\"pc\":191,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0xf\",\"0x184\"]},{\"pc\":192,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0x184\",\"0xf\"]},{\"pc\":193,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\",\"0x184\"]},{\"pc\":388,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":389,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":390,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0xf\"]},{\"pc\":391,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":394,\"op\":\"DUP4\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\"]},{\"pc\":395,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":398,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0xb8\"]},{\"pc\":184,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":185,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\"]},{\"pc\":187,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x0\"]},{\"pc\":188,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x0\",\"0x1\"]},{\"pc\":189,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x1\",\"0x0\"]},{\"pc\":190,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x18f\",\"0x1\",\"0x1\"]},{\"pc\":191,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x1\",\"0x18f\"]},{\"pc\":192,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x18f\",\"0x1\"]},{\"pc\":193,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0x18f\"]},{\"pc\":399,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":400,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":401,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":402,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\"]},{\"pc\":403,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\"]},{\"pc\":404,\"op\":\"ADD\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x1\",\"0xf\"]},{\"pc\":405,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x0\",\"0x10\"]},{\"pc\":406,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x0\"]},{\"pc\":407,\"op\":\"DUP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":408,\"op\":\"DUP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x10\"]},{\"pc\":409,\"op\":\"GT\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x10\",\"0xf\"]},{\"pc\":410,\"op\":\"ISZERO\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x0\"]},{\"pc\":411,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x1\"]},{\"pc\":414,\"op\":\"JUMPI\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\",\"0x1\",\"0x1a7\"]},{\"pc\":423,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":424,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0xaf\",\"0x1\",\"0xf\",\"0x10\"]},{\"pc\":425,\"op\":\"SWAP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0x1\",\"0xf\",\"0xaf\"]},{\"pc\":426,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\",\"0xf\",\"0x1\"]},{\"pc\":427,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\",\"0xf\"]},{\"pc\":428,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\",\"0xaf\"]},{\"pc\":175,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\"]},{\"pc\":176,\"op\":\"SWAP3\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x1\",\"0x0\",\"0x0\",\"0x10\"]},{\"pc\":177,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x0\",\"0x1\"]},{\"pc\":178,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x0\"]},{\"pc\":179,\"op\":\"DUP2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\"]},{\"pc\":180,\"op\":\"SWAP1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x0\",\"0x10\"]},{\"pc\":181,\"op\":\"SSTORE\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\",\"0x10\",\"0x0\"],\"storage\":{\"0000000000000000000000000000000000000000000000000000000000000000\":\"0000000000000000000000000000000000000000000000000000000000000010\"}},{\"pc\":182,\"op\":\"POP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\",\"0x10\"]},{\"pc\":183,\"op\":\"JUMP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\",\"0x88\"]},{\"pc\":136,\"op\":\"JUMPDEST\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]},{\"pc\":137,\"op\":\"STOP\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0xd09de08a\"]}]}").await;
}

#[tokio::test]
async fn test_trace_state_diff_increment_call() {
    trace_increment_call(helpers::state_diff_trace_config(), "{\"output\":\"0x\",\"stateDiff\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":\"=\",\"nonce\":\"=\",\"code\":\"=\",\"storage\":{\"0x0000000000000000000000000000000000000000000000000000000000000000\":{\"*\":{\"from\":\"0x000000000000000000000000000000000000000000000000000000000000000f\",\"to\":\"0x0000000000000000000000000000000000000000000000000000000000000010\"}}}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":{\"*\":{\"from\":\"0x4692885e28cc02260\",\"to\":\"0x4691bba948aba7cc0\"}},\"nonce\":{\"*\":{\"from\":\"0x7\",\"to\":\"0x8\"}},\"code\":\"=\",\"storage\":{}}},\"trace\":[],\"vmTrace\":null}").await;
}

#[tokio::test]
async fn test_trace_prestate_increment_call() {
    trace_increment_call(helpers::prestate_trace_config(), "{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":\"0x0\",\"code\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"nonce\":1,\"storage\":{\"0x0000000000000000000000000000000000000000000000000000000000000000\":\"0x000000000000000000000000000000000000000000000000000000000000000f\"}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":\"0x4692885e28cc02260\",\"nonce\":7}}").await;
}

#[tokio::test]
async fn test_trace_prestate_diff_mode_increment_call() {
    trace_increment_call(helpers::prestate_diff_mode_trace_config(), "{\"post\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"storage\":{\"0x0000000000000000000000000000000000000000000000000000000000000000\":\"0x0000000000000000000000000000000000000000000000000000000000000010\"}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":\"0x4691bba948aba7cc0\",\"nonce\":8}},\"pre\":{\"0x356726f027a805fab3bd7dd0413a96d81bc6f599\":{\"balance\":\"0x0\",\"code\":\"0x608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033\",\"nonce\":1,\"storage\":{\"0x0000000000000000000000000000000000000000000000000000000000000000\":\"0x000000000000000000000000000000000000000000000000000000000000000f\"}},\"0x82211934c340b29561381392348d48413e15adc8\":{\"balance\":\"0x4692885e28cc02260\",\"nonce\":7}}}").await;
}

// tx_hash: 0xf1a8130526ff5951a8d1c7e31623f23ec84d8644514e5513a440e139a30f5166
async fn trace_increment_call(trace_config: TraceConfig, expected_trace: &str) {
    let gas_used = Some(U256::from(10_000u64));
    let chain_id = 1234;

    let tracer = new_tracer(gas_used, trace_config).unwrap();

    let origin = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();
    let target = Address::from_hex("0x356726f027a805fab3bd7dd0413a96d81bc6f599").unwrap();

    let index = U256::ZERO;

    let program_id = Pubkey::new_unique();

    let rpc = increment_call_test_rpc(program_id, origin, target, index, chain_id).await;

    let mut test_account_storage =
        helpers::test_emulator_account_storage(program_id, &rpc, chain_id).await;

    let trx = increment_tx_params(gas_used, origin, target).await;

    let mut backend = ExecutorState::new(&mut test_account_storage);

    let emulate_response = emulate_trx(trx, &mut backend, 1000, Some(Rc::clone(&tracer)))
        .await
        .unwrap();

    assert_eq!(emulate_response.exit_status, "succeed"); // todo why stop?
    assert_eq!(emulate_response.result, Vec::<u8>::new());
    assert_eq!(emulate_response.steps_executed, 112);
    assert_eq!(emulate_response.used_gas, 25_000);
    assert_eq!(emulate_response.iterations, 3);
    assert_eq!(emulate_response.solana_accounts.len(), 2);

    let result = into_traces(tracer, emulate_response);

    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        expected_trace.to_string()
    );

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
) -> impl Rpc + BuildConfigSimulator {
    let (origin_pubkey, origin_account) = helpers::balance_account_with_data(
        &program_id,
        origin,
        chain_id,
        7,
        U256::from_str_hex("0x4692885e28cc02260").unwrap(),
    );

    let (target_pubkey, target_account) =
        helpers::balance_account_with_data(&program_id, target, chain_id, 1, U256::ZERO);

    let code_vec = hex::decode("608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033").unwrap();

    let (target_code_pubkey, _) = target.find_solana_address(&program_id);
    let mut target_code_account = Account::new(
        0,
        ContractAccount::required_account_size(&code_vec),
        &program_id,
    );
    let code_account_info =
        helpers::writable_account_info(&target_code_pubkey, &mut target_code_account);
    let mut contract = ContractAccount::new(
        &program_id,
        code_account_info,
        target,
        chain_id,
        0,
        &code_vec,
    )
    .unwrap();

    contract.set_storage_value(
        index.as_usize(),
        &U256::from_str_hex("0x0f").unwrap().to_be_bytes(),
    );

    TestRpc::new(hash_map! {
        origin_pubkey => origin_account,
        target_pubkey => target_account,
        target_code_pubkey => target_code_account,
        solana_sdk::sysvar::rent::id() => rent_account()
    })
}
