#![allow(clippy::trait_duplication_in_bounds)]
#![allow(clippy::type_repetition_in_bounds)]
#![allow(clippy::unsafe_derive_deserialize)]

use std::{marker::PhantomData, ops::Range};

use ethnum::U256;
use maybe_async::maybe_async;
use serde::{Deserialize, Serialize};
use solana_program::log::sol_log_data;

pub use buffer::Buffer;

#[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
use crate::evm::tracing::TracerTypeOpt;
use crate::{
    error::{build_revert_message, Error, Result},
    evm::{opcode::Action, precompile::is_precompile_address},
    types::{Address, Transaction},
};

use self::{database::Database, memory::Memory, stack::Stack};

mod buffer;
pub mod database;
mod memory;
mod opcode;
pub mod opcode_table;
mod precompile;
mod stack;
#[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
pub mod tracing;
mod utils;

macro_rules! tracing_event {
    ($self:ident, $x:expr) => {
        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
        if let Some(tracer) = &mut $self.tracer {
            tracer.event($x);
        }
    };
    ($self:ident, $condition:expr, $x:expr) => {
        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
        if let Some(tracer) = &mut $self.tracer {
            if $condition {
                tracer.event($x);
            }
        }
    };
}

macro_rules! trace_end_step {
    ($self:ident, $return_data:expr) => {
        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
        if let Some(tracer) = &mut $self.tracer {
            tracer.event(crate::evm::tracing::Event::EndStep {
                gas_used: 0_u64,
                return_data: $return_data,
            })
        }
    };
    ($self:ident, $condition:expr; $return_data_getter:expr) => {
        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
        if $condition {
            trace_end_step!($self, $return_data_getter)
        }
    };
}

pub(crate) use trace_end_step;
pub(crate) use tracing_event;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExitStatus {
    Stop,
    Return(#[serde(with = "serde_bytes")] Vec<u8>),
    Revert(#[serde(with = "serde_bytes")] Vec<u8>),
    Suicide,
    StepLimit,
}

impl ExitStatus {
    #[must_use]
    pub fn status(&self) -> &'static str {
        match self {
            ExitStatus::Return(_) | ExitStatus::Stop | ExitStatus::Suicide => "succeed",
            ExitStatus::Revert(_) => "revert",
            ExitStatus::StepLimit => "step limit exceeded",
        }
    }

    #[must_use]
    pub fn is_succeed(&self) -> Option<bool> {
        match self {
            ExitStatus::Stop | ExitStatus::Return(_) | ExitStatus::Suicide => Some(true),
            ExitStatus::Revert(_) => Some(false),
            ExitStatus::StepLimit => None,
        }
    }

    #[must_use]
    pub fn into_result(self) -> Option<Vec<u8>> {
        match self {
            ExitStatus::Return(v) | ExitStatus::Revert(v) => Some(v),
            ExitStatus::Stop | ExitStatus::Suicide | ExitStatus::StepLimit => None,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Reason {
    Call,
    Create,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Context {
    pub caller: Address,
    pub contract: Address,
    #[serde(with = "ethnum::serde::bytes::le")]
    pub value: U256,

    pub code_address: Option<Address>,
}

// #[derive(Debug, PartialEq)]
pub struct MachineResult {
    pub exit_status: ExitStatus,
    pub steps_executed: u64,
    #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
    pub tracer: TracerTypeOpt,
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "B: Database")]
pub struct Machine<B: Database> {
    origin: Address,
    context: Context,

    #[serde(with = "ethnum::serde::bytes::le")]
    gas_price: U256,
    #[serde(with = "ethnum::serde::bytes::le")]
    gas_limit: U256,

    execution_code: Buffer,
    call_data: Buffer,
    return_data: Buffer,
    return_range: Range<usize>,

    stack: Stack,
    memory: Memory,
    pc: usize,

    is_static: bool,
    reason: Reason,

    parent: Option<Box<Self>>,

    #[serde(skip)]
    phantom: PhantomData<*const B>,

    #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
    #[serde(skip)]
    tracer: TracerTypeOpt,
}

impl<B: Database> Machine<B> {
    pub fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize> {
        let mut cursor = std::io::Cursor::new(buffer);

        bincode::serialize_into(&mut cursor, &self)?;

        cursor.position().try_into().map_err(Error::from)
    }

    #[cfg(any(target_os = "solana", feature = "test-bpf"))]
    pub fn deserialize_from(buffer: &[u8], backend: &B) -> Result<Self> {
        fn reinit_buffer<B: Database>(buffer: &mut Buffer, backend: &B) {
            if let Some((key, range)) = buffer.uninit_data() {
                *buffer =
                    backend.map_solana_account(&key, |i| unsafe { Buffer::from_account(i, range) });
            }
        }

        fn reinit_machine<B: Database>(mut machine: &mut Machine<B>, backend: &B) {
            loop {
                reinit_buffer(&mut machine.call_data, backend);
                reinit_buffer(&mut machine.execution_code, backend);
                reinit_buffer(&mut machine.return_data, backend);

                match &mut machine.parent {
                    None => break,
                    Some(parent) => machine = parent,
                }
            }
        }

        let mut evm: Self = bincode::deserialize(buffer)?;
        reinit_machine(&mut evm, backend);

        Ok(evm)
    }

    #[maybe_async]
    pub async fn new(
        trx: &mut Transaction,
        origin: Address,
        backend: &mut B,
        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))] tracer: TracerTypeOpt,
    ) -> Result<Self> {
        let origin_nonce = backend.nonce(&origin).await?;

        if origin_nonce == u64::MAX {
            return Err(Error::NonceOverflow(origin));
        }

        if origin_nonce != trx.nonce() {
            return Err(Error::InvalidTransactionNonce(
                origin,
                origin_nonce,
                trx.nonce(),
            ));
        }

        if let Some(chain_id) = trx.chain_id() {
            if backend.chain_id() != chain_id {
                return Err(Error::InvalidChainId(chain_id));
            }
        }

        if backend.balance(&origin).await? < trx.value() {
            return Err(Error::InsufficientBalance(origin, trx.value()));
        }

        if backend.code_size(&origin).await? != 0 {
            return Err(Error::SenderHasDeployedCode(origin));
        }

        if trx.target().is_some() {
            Self::new_call(
                trx,
                origin,
                backend,
                #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
                tracer,
            )
            .await
        } else {
            Self::new_create(
                trx,
                origin,
                backend,
                #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
                tracer,
            )
            .await
        }
    }

    #[maybe_async]
    async fn new_call(
        trx: &mut Transaction,
        origin: Address,
        backend: &mut B,
        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))] tracer: TracerTypeOpt,
    ) -> Result<Self> {
        assert!(trx.target().is_some());

        let target = trx.target().unwrap();
        sol_log_data(&[b"ENTER", b"CALL", target.as_bytes()]);

        backend.increment_nonce(origin)?;
        backend.snapshot();

        backend.transfer(origin, target, trx.value()).await?;

        let execution_code = backend.code(&target).await?;

        Ok(Self {
            origin,
            context: Context {
                caller: origin,
                contract: target,
                value: trx.value(),
                code_address: Some(target),
            },
            gas_price: trx.gas_price(),
            gas_limit: trx.gas_limit(),
            execution_code,
            call_data: trx.extract_call_data(),
            return_data: Buffer::empty(),
            return_range: 0..0,
            stack: Stack::new(),
            memory: Memory::new(),
            pc: 0_usize,
            is_static: false,
            reason: Reason::Call,
            parent: None,
            phantom: PhantomData,
            #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
            tracer,
        })
    }

    #[maybe_async]
    async fn new_create(
        trx: &mut Transaction,
        origin: Address,
        backend: &mut B,
        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))] tracer: TracerTypeOpt,
    ) -> Result<Self> {
        assert!(trx.target().is_none());

        let target = Address::from_create(&origin, trx.nonce());
        sol_log_data(&[b"ENTER", b"CREATE", target.as_bytes()]);

        if (backend.nonce(&target).await? != 0) || (backend.code_size(&target).await? != 0) {
            return Err(Error::DeployToExistingAccount(target, origin));
        }

        backend.increment_nonce(origin)?;
        backend.snapshot();

        backend.increment_nonce(target)?;
        backend.transfer(origin, target, trx.value()).await?;

        Ok(Self {
            origin,
            context: Context {
                caller: origin,
                contract: target,
                value: trx.value(),
                code_address: None,
            },
            gas_price: trx.gas_price(),
            gas_limit: trx.gas_limit(),
            return_data: Buffer::empty(),
            return_range: 0..0,
            stack: Stack::new(),
            memory: Memory::new(),
            pc: 0_usize,
            is_static: false,
            reason: Reason::Create,
            execution_code: trx.extract_call_data(),
            call_data: Buffer::empty(),
            parent: None,
            phantom: PhantomData,
            #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
            tracer,
        })
    }

    #[maybe_async]
    pub async fn execute(&mut self, step_limit: u64, backend: &mut B) -> Result<MachineResult> {
        assert!(self.execution_code.is_initialized());
        assert!(self.call_data.is_initialized());
        assert!(self.return_data.is_initialized());

        let mut step = 0_u64;

        tracing_event!(
            self,
            tracing::Event::BeginVM {
                context: self.context,
                code: self.execution_code.to_vec()
            }
        );

        let status = if is_precompile_address(&self.context.contract) {
            let value = Self::precompile(&self.context.contract, &self.call_data).unwrap();
            backend.commit_snapshot();

            ExitStatus::Return(value)
        } else {
            loop {
                step += 1;
                if step > step_limit {
                    break ExitStatus::StepLimit;
                }

                let opcode = self.execution_code.get_or_default(self.pc);

                tracing_event!(
                    self,
                    tracing::Event::BeginStep {
                        opcode,
                        pc: self.pc,
                        stack: self.stack.to_vec(),
                        memory: self.memory.to_vec()
                    }
                );

                // let _opname = crate::evm::opcode_table::OPNAMES[opcode as usize];

                let opcode_result = match self.execute_opcode(backend, opcode).await {
                    Ok(result) => result,
                    Err(e) => {
                        let message = build_revert_message(&e.to_string());
                        self.opcode_revert_impl(Buffer::from_slice(&message), backend)
                            .await?
                    }
                };

                trace_end_step!(self, opcode_result != Action::Noop; match &opcode_result {
                    Action::Return(value) | Action::Revert(value) => Some(value.clone()),
                    _ => None,
                });

                match opcode_result {
                    Action::Continue => self.pc += 1,
                    Action::Jump(target) => self.pc = target,
                    Action::Stop => break ExitStatus::Stop,
                    Action::Return(value) => break ExitStatus::Return(value),
                    Action::Revert(value) => break ExitStatus::Revert(value),
                    Action::Suicide => break ExitStatus::Suicide,
                    Action::Noop => {}
                };
            }
        };

        tracing_event!(
            self,
            tracing::Event::EndVM {
                status: status.clone()
            }
        );

        Ok(MachineResult {
            exit_status: status,
            steps_executed: step,
            #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
            tracer: self.tracer.take(),
        })
    }

    fn fork(
        &mut self,
        reason: Reason,
        context: Context,
        execution_code: Buffer,
        call_data: Buffer,
        gas_limit: Option<U256>,
    ) {
        let mut other = Self {
            origin: self.origin,
            context,
            gas_price: self.gas_price,
            gas_limit: gas_limit.unwrap_or(self.gas_limit),
            execution_code,
            call_data,
            return_data: Buffer::empty(),
            return_range: 0..0,
            stack: Stack::new(),
            memory: Memory::new(),
            pc: 0_usize,
            is_static: self.is_static,
            reason,
            parent: None,
            phantom: PhantomData,
            #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
            tracer: self.tracer.take(),
        };

        core::mem::swap(self, &mut other);
        self.parent = Some(Box::new(other));
    }

    fn join(&mut self) -> Self {
        assert!(self.parent.is_some());

        let mut other = *self.parent.take().unwrap();

        #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
        {
            other.tracer = self.tracer.take();
        }

        core::mem::swap(self, &mut other);

        other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::ether_account;
    use crate::account_storage::AccountStorage;
    use crate::executor::{ExecutorState, OwnedAccountInfo};
    use solana_program::account_info::AccountInfo;
    use solana_program::pubkey::Pubkey;
    use std::collections::HashMap;

    struct TestAccountStorage {
        chain_id: u64,
        block_number: U256,
        block_timestamp: U256,
        accounts: HashMap<Address, ether_account::Data>,
    }

    #[maybe_async(?Send)]
    impl AccountStorage for TestAccountStorage {
        fn all_addresses(&self) -> Vec<Address> {
            todo!()
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

        async fn block_hash(&self, _number: u64) -> [u8; 32] {
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

        async fn code_hash(&mut self, _address: &Address) -> [u8; 32] {
            todo!()
        }

        async fn code(&mut self, _address: &Address) -> Buffer {
            todo!()
        }

        async fn generation(&mut self, _address: &Address) -> u32 {
            todo!()
        }

        async fn storage(&mut self, _address: &Address, _index: &U256) -> [u8; 32] {
            todo!()
        }

        async fn clone_solana_account(&self, _address: &Pubkey) -> OwnedAccountInfo {
            todo!()
        }

        async fn map_solana_account<F, R>(&self, _address: &Pubkey, _action: F) -> R
        where
            F: FnOnce(&AccountInfo) -> R,
        {
            todo!()
        }

        async fn solana_account_space(&mut self, _address: &Address) -> Option<usize> {
            todo!()
        }
    }

    #[maybe_async::test(feature = "test-bpf", async(not(feature = "test-bpf"), tokio::test))]
    async fn test_contract_creation() {
        let address = Address::from_hex("0x82211934c340b29561381392348d48413e15adc8").unwrap();

        let chain_id = 123u64;
        let nonce = 0;

        let input_data = hex::decode("608060405234801561001057600080fd5b506101e3806100206000396000f3fe608060405234801561001057600080fd5b50600436106100415760003560e01c80632e64cec1146100465780636057361d14610064578063d09de08a14610080575b600080fd5b61004e61008a565b60405161005b91906100d1565b60405180910390f35b61007e6004803603810190610079919061011d565b610093565b005b61008861009d565b005b60008054905090565b8060008190555050565b60016000808282546100af9190610179565b92505081905550565b6000819050919050565b6100cb816100b8565b82525050565b60006020820190506100e660008301846100c2565b92915050565b600080fd5b6100fa816100b8565b811461010557600080fd5b50565b600081359050610117816100f1565b92915050565b600060208284031215610133576101326100ec565b5b600061014184828501610108565b91505092915050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000610184826100b8565b915061018f836100b8565b92508282019050808211156101a7576101a661014a565b5b9291505056fea2646970667358221220ebb58b4c4a532694d88df38fd2089943f69980725510d4814d3bd8ccf0c4717464736f6c63430008120033").unwrap();

        let mut trx = Transaction {
            transaction: crate::types::TransactionPayload::Legacy(crate::types::LegacyTx {
                nonce,
                gas_price: U256::ZERO,
                gas_limit: U256::MAX,
                target: None,
                value: U256::ZERO,
                call_data: Buffer::from_slice(&input_data),
                v: U256::default(),
                r: U256::default(),
                s: U256::default(),
                chain_id: Some(U256::from(chain_id)),
                recovery_id: u8::default(),
            }),
            byte_len: usize::default(),
            hash: <[u8; 32]>::default(),
            signed_hash: <[u8; 32]>::default(),
        };

        let mut accounts = HashMap::new();
        accounts.insert(
            address,
            ether_account::Data {
                address,
                bump_seed: 0,
                trx_count: nonce,
                balance: U256::default(),
                generation: 0,
                code_size: 0,
                rw_blocked: false,
            },
        );

        let mut storage = TestAccountStorage {
            chain_id,
            block_number: U256::ZERO,
            block_timestamp: U256::ZERO,
            accounts,
        };

        let mut backend = ExecutorState::new(&mut storage);

        let mut machine = Machine::new(
            &mut trx,
            address,
            &mut backend,
            #[cfg(all(not(target_os = "solana"), not(feature = "test-bpf")))]
            None,
        )
        .await
        .unwrap();

        let result = machine.execute(1000, &mut backend).await.unwrap();

        assert_eq!(
            (result.exit_status, result.steps_executed),
            (
                ExitStatus::Return(input_data.into_iter().skip(32).collect()),
                17
            )
        );
    }
}
