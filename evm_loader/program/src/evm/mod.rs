#![allow(clippy::trait_duplication_in_bounds)]
#![allow(clippy::type_repetition_in_bounds)]
#![allow(clippy::unsafe_derive_deserialize)]

use std::{marker::PhantomData, ops::Range};

use ethnum::U256;
use serde::{Deserialize, Serialize};
use solana_program::log::sol_log_data;

pub use buffer::Buffer;
pub use precompile::is_precompile_address;
pub use precompile::precompile;

#[cfg(feature = "tracing")]
use crate::evm::tracing::event_listener::tracer::TracerType;
#[cfg(feature = "tracing")]
use crate::evm::tracing::EventListener;
use crate::{
    error::{build_revert_message, Error, Result},
    evm::opcode::Action,
    types::{Address, Transaction},
};

use self::{database::Database, memory::Memory, stack::Stack};

mod buffer;
pub mod database;
mod memory;
mod opcode;
mod precompile;
mod stack;
#[cfg(feature = "tracing")]
pub mod tracing;
mod utils;

macro_rules! tracing_event {
    ($self:ident, $x:expr) => {
        #[cfg(feature = "tracing")]
        if let Some(tracer) = &$self.tracer {
            tracer.write().unwrap().as_mut().unwrap().event($x);
        }
    };
    ($self:ident, $condition:expr, $x:expr) => {
        #[cfg(feature = "tracing")]
        if let Some(tracer) = &$self.tracer {
            if $condition {
                tracer.write().unwrap().as_mut().unwrap().event($x);
            }
        }
    };
}

macro_rules! trace_end_step {
    ($self:ident, $return_data_vec:expr) => {
        #[cfg(feature = "tracing")]
        if let Some(tracer) = &$self.tracer {
            let mut tracer = tracer.write().unwrap();
            let tracer = tracer.as_mut().unwrap();
            if tracer.enable_return_data() {
                tracer.event(crate::evm::tracing::Event::EndStep {
                    gas_used: 0_u64,
                    return_data: $return_data_vec,
                })
            } else {
                tracer.event(crate::evm::tracing::Event::EndStep {
                    gas_used: 0_u64,
                    return_data: None,
                })
            }
        }
    };
    ($self:ident, $condition:expr; $return_data_vec:expr) => {
        #[cfg(feature = "tracing")]
        if $condition {
            trace_end_step!($self, $return_data_vec)
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

    #[serde(skip)]
    #[cfg(feature = "tracing")]
    tracer: TracerType,
}

impl<B: Database> Machine<B> {
    pub fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize> {
        let mut cursor = std::io::Cursor::new(buffer);

        bincode::serialize_into(&mut cursor, &self)?;

        cursor.position().try_into().map_err(Error::from)
    }

    pub fn deserialize_from(buffer: &[u8], backend: &B) -> Result<Self> {
        fn reinit_buffer<B: Database>(buffer: &mut Buffer, backend: &B) {
            if let Some((key, range)) = buffer.uninit_data() {
                *buffer = backend.map_solana_account(&key, |i| Buffer::from_account(i, range));
            }
        }

        fn reinit_machine<B: Database>(machine: &mut Machine<B>, backend: &B) {
            reinit_buffer(&mut machine.call_data, backend);
            reinit_buffer(&mut machine.execution_code, backend);
            reinit_buffer(&mut machine.return_data, backend);

            if let Some(parent) = &mut machine.parent {
                reinit_machine(parent, backend);
            }
        }

        let mut evm: Self = bincode::deserialize(buffer)?;
        reinit_machine(&mut evm, backend);

        Ok(evm)
    }

    #[maybe_async::maybe_async]
    pub async fn new(
        trx: Transaction,
        origin: Address,
        backend: &mut B,
        #[cfg(feature = "tracing")] tracer: TracerType,
    ) -> Result<Self> {
        let origin_nonce = backend.nonce(&origin)?;

        if origin_nonce == u64::MAX {
            return Err(Error::NonceOverflow(origin));
        }

        if origin_nonce != trx.nonce {
            return Err(Error::InvalidTransactionNonce(
                origin,
                origin_nonce,
                trx.nonce,
            ));
        }

        if let Some(chain_id) = trx.chain_id {
            if backend.chain_id() != chain_id {
                return Err(Error::InvalidChainId(chain_id));
            }
        }

        if backend.balance(&origin)? < trx.value {
            return Err(Error::InsufficientBalance(origin, trx.value));
        }

        if backend.code_size(&origin)? != 0 {
            return Err(Error::SenderHasDeployedCode(origin));
        }

        if trx.target.is_some() {
            Self::new_call(
                trx,
                origin,
                backend,
                #[cfg(feature = "tracing")]
                tracer,
            )
        } else {
            Self::new_create(
                trx,
                origin,
                backend,
                #[cfg(feature = "tracing")]
                tracer,
            )
        }
    }

    fn new_call(
        trx: Transaction,
        origin: Address,
        backend: &mut B,
        #[cfg(feature = "tracing")] tracer: TracerType,
    ) -> Result<Self> {
        assert!(trx.target.is_some());

        let target = trx.target.unwrap();
        sol_log_data(&[b"ENTER", b"CALL", target.as_bytes()]);

        backend.increment_nonce(origin)?;
        backend.snapshot();

        backend.transfer(origin, target, trx.value)?;

        let execution_code = backend.code(&target)?;

        Ok(Self {
            origin,
            context: Context {
                caller: origin,
                contract: target,
                value: trx.value,
                code_address: Some(target),
            },
            gas_price: trx.gas_price,
            gas_limit: trx.gas_limit,
            execution_code,
            call_data: trx.call_data,
            return_data: Buffer::empty(),
            return_range: 0..0,
            stack: Stack::new(
                #[cfg(feature = "tracing")]
                tracer.clone(),
            ),
            memory: Memory::new(
                #[cfg(feature = "tracing")]
                tracer.clone(),
            ),
            pc: 0_usize,
            is_static: false,
            reason: Reason::Call,
            parent: None,
            phantom: PhantomData,
            #[cfg(feature = "tracing")]
            tracer,
        })
    }

    fn new_create(
        trx: Transaction,
        origin: Address,
        backend: &mut B,
        #[cfg(feature = "tracing")] tracer: TracerType,
    ) -> Result<Self> {
        assert!(trx.target.is_none());

        let target = Address::from_create(&origin, trx.nonce);
        sol_log_data(&[b"ENTER", b"CREATE", target.as_bytes()]);

        if (backend.nonce(&target)? != 0) || (backend.code_size(&target)? != 0) {
            return Err(Error::DeployToExistingAccount(target, origin));
        }

        backend.increment_nonce(origin)?;
        backend.snapshot();

        backend.increment_nonce(target)?;
        backend.transfer(origin, target, trx.value)?;

        Ok(Self {
            origin,
            context: Context {
                caller: origin,
                contract: target,
                value: trx.value,
                code_address: None,
            },
            gas_price: trx.gas_price,
            gas_limit: trx.gas_limit,
            return_data: Buffer::empty(),
            return_range: 0..0,
            stack: Stack::new(
                #[cfg(feature = "tracing")]
                tracer.clone(),
            ),
            memory: Memory::new(
                #[cfg(feature = "tracing")]
                tracer.clone(),
            ),
            pc: 0_usize,
            is_static: false,
            reason: Reason::Create,
            execution_code: trx.call_data,
            call_data: Buffer::empty(),
            parent: None,
            phantom: PhantomData,
            #[cfg(feature = "tracing")]
            tracer,
        })
    }

    #[maybe_async::maybe_async]
    pub async fn execute(&mut self, step_limit: u64, backend: &mut B) -> Result<(ExitStatus, u64)> {
        assert!(self.execution_code.uninit_data().is_none());
        assert!(self.call_data.uninit_data().is_none());
        assert!(self.return_data.uninit_data().is_none());

        let mut step = 0_u64;

        tracing_event!(
            self,
            tracing::Event::BeginVM {
                context: self.context,
                code: self.execution_code.to_vec()
            }
        );

        let status = loop {
            if is_precompile_address(&self.context.contract) {
                let value = precompile(&self.context.contract, &self.call_data).unwrap_or_default();

                backend.commit_snapshot();

                break ExitStatus::Return(value);
            }

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

            let opcode_result = match match opcode {
                0x00 => self.opcode_stop(backend),
                0x01 => self.opcode_add(backend),
                0x02 => self.opcode_mul(backend),
                0x03 => self.opcode_sub(backend),
                0x04 => self.opcode_div(backend),
                0x05 => self.opcode_sdiv(backend),
                0x06 => self.opcode_mod(backend),
                0x07 => self.opcode_smod(backend),
                0x08 => self.opcode_addmod(backend),
                0x09 => self.opcode_mulmod(backend),
                0x0A => self.opcode_exp(backend),
                0x0B => self.opcode_signextend(backend),

                0x10 => self.opcode_lt(backend),
                0x11 => self.opcode_gt(backend),
                0x12 => self.opcode_slt(backend),
                0x13 => self.opcode_sgt(backend),
                0x14 => self.opcode_eq(backend),
                0x15 => self.opcode_iszero(backend),
                0x16 => self.opcode_and(backend),
                0x17 => self.opcode_or(backend),
                0x18 => self.opcode_xor(backend),
                0x19 => self.opcode_not(backend),
                0x1A => self.opcode_byte(backend),
                0x1B => self.opcode_shl(backend),
                0x1C => self.opcode_shr(backend),
                0x1D => self.opcode_sar(backend),

                0x20 => self.opcode_sha3(backend),

                0x30 => self.opcode_address(backend),
                0x31 => self.opcode_balance(backend),
                0x32 => self.opcode_origin(backend),
                0x33 => self.opcode_caller(backend),
                0x34 => self.opcode_callvalue(backend),
                0x35 => self.opcode_calldataload(backend),
                0x36 => self.opcode_calldatasize(backend),
                0x37 => self.opcode_calldatacopy(backend),
                0x38 => self.opcode_codesize(backend),
                0x39 => self.opcode_codecopy(backend),
                0x3A => self.opcode_gasprice(backend),
                0x3B => self.opcode_extcodesize(backend),
                0x3C => self.opcode_extcodecopy(backend),
                0x3D => self.opcode_returndatasize(backend),
                0x3E => self.opcode_returndatacopy(backend),
                0x3F => self.opcode_extcodehash(backend),
                0x40 => self.opcode_blockhash(backend).await,
                0x41 => self.opcode_coinbase(backend),
                0x42 => self.opcode_timestamp(backend),
                0x43 => self.opcode_number(backend),
                0x44 => self.opcode_difficulty(backend),
                0x45 => self.opcode_gaslimit(backend),
                0x46 => self.opcode_chainid(backend),
                0x47 => self.opcode_selfbalance(backend),
                0x48 => self.opcode_basefee(backend),

                0x50 => self.opcode_pop(backend),
                0x51 => self.opcode_mload(backend),
                0x52 => self.opcode_mstore(backend),
                0x53 => self.opcode_mstore8(backend),
                0x54 => self.opcode_sload(backend),
                0x55 => self.opcode_sstore(backend),
                0x56 => self.opcode_jump(backend),
                0x57 => self.opcode_jumpi(backend),
                0x58 => self.opcode_pc(backend),
                0x59 => self.opcode_msize(backend),
                0x5A => self.opcode_gas(backend),
                0x5B => self.opcode_jumpdest(backend),

                0x5F => self.opcode_push_0(backend),
                0x60 => self.opcode_push_1(backend),
                0x61 => self.opcode_push_2_31::<2>(backend),
                0x62 => self.opcode_push_2_31::<3>(backend),
                0x63 => self.opcode_push_2_31::<4>(backend),
                0x64 => self.opcode_push_2_31::<5>(backend),
                0x65 => self.opcode_push_2_31::<6>(backend),
                0x66 => self.opcode_push_2_31::<7>(backend),
                0x67 => self.opcode_push_2_31::<8>(backend),
                0x68 => self.opcode_push_2_31::<9>(backend),
                0x69 => self.opcode_push_2_31::<10>(backend),
                0x6A => self.opcode_push_2_31::<11>(backend),
                0x6B => self.opcode_push_2_31::<12>(backend),
                0x6C => self.opcode_push_2_31::<13>(backend),
                0x6D => self.opcode_push_2_31::<14>(backend),
                0x6E => self.opcode_push_2_31::<15>(backend),
                0x6F => self.opcode_push_2_31::<16>(backend),
                0x70 => self.opcode_push_2_31::<17>(backend),
                0x71 => self.opcode_push_2_31::<18>(backend),
                0x72 => self.opcode_push_2_31::<19>(backend),
                0x73 => self.opcode_push_2_31::<20>(backend),
                0x74 => self.opcode_push_2_31::<21>(backend),
                0x75 => self.opcode_push_2_31::<22>(backend),
                0x76 => self.opcode_push_2_31::<23>(backend),
                0x77 => self.opcode_push_2_31::<24>(backend),
                0x78 => self.opcode_push_2_31::<25>(backend),
                0x79 => self.opcode_push_2_31::<26>(backend),
                0x7A => self.opcode_push_2_31::<27>(backend),
                0x7B => self.opcode_push_2_31::<28>(backend),
                0x7C => self.opcode_push_2_31::<29>(backend),
                0x7D => self.opcode_push_2_31::<30>(backend),
                0x7E => self.opcode_push_2_31::<31>(backend),
                0x7F => self.opcode_push_32(backend),

                0x80 => self.opcode_dup_1_16::<1>(backend),
                0x81 => self.opcode_dup_1_16::<2>(backend),
                0x82 => self.opcode_dup_1_16::<3>(backend),
                0x83 => self.opcode_dup_1_16::<4>(backend),
                0x84 => self.opcode_dup_1_16::<5>(backend),
                0x85 => self.opcode_dup_1_16::<6>(backend),
                0x86 => self.opcode_dup_1_16::<7>(backend),
                0x87 => self.opcode_dup_1_16::<8>(backend),
                0x88 => self.opcode_dup_1_16::<9>(backend),
                0x89 => self.opcode_dup_1_16::<10>(backend),
                0x8A => self.opcode_dup_1_16::<11>(backend),
                0x8B => self.opcode_dup_1_16::<12>(backend),
                0x8C => self.opcode_dup_1_16::<13>(backend),
                0x8D => self.opcode_dup_1_16::<14>(backend),
                0x8E => self.opcode_dup_1_16::<15>(backend),
                0x8F => self.opcode_dup_1_16::<16>(backend),

                0x90 => self.opcode_swap_1_16::<1>(backend),
                0x91 => self.opcode_swap_1_16::<2>(backend),
                0x92 => self.opcode_swap_1_16::<3>(backend),
                0x93 => self.opcode_swap_1_16::<4>(backend),
                0x94 => self.opcode_swap_1_16::<5>(backend),
                0x95 => self.opcode_swap_1_16::<6>(backend),
                0x96 => self.opcode_swap_1_16::<7>(backend),
                0x97 => self.opcode_swap_1_16::<8>(backend),
                0x98 => self.opcode_swap_1_16::<9>(backend),
                0x99 => self.opcode_swap_1_16::<10>(backend),
                0x9A => self.opcode_swap_1_16::<11>(backend),
                0x9B => self.opcode_swap_1_16::<12>(backend),
                0x9C => self.opcode_swap_1_16::<13>(backend),
                0x9D => self.opcode_swap_1_16::<14>(backend),
                0x9E => self.opcode_swap_1_16::<15>(backend),
                0x9F => self.opcode_swap_1_16::<16>(backend),

                0xA0 => self.opcode_log_0_4::<0>(backend),
                0xA1 => self.opcode_log_0_4::<1>(backend),
                0xA2 => self.opcode_log_0_4::<2>(backend),
                0xA3 => self.opcode_log_0_4::<3>(backend),
                0xA4 => self.opcode_log_0_4::<4>(backend),

                0xF0 => self.opcode_create(backend),
                0xF1 => self.opcode_call(backend),
                0xF2 => self.opcode_callcode(backend),
                0xF3 => self.opcode_return(backend),
                0xF4 => self.opcode_delegatecall(backend),
                0xF5 => self.opcode_create2(backend),

                0xFA => self.opcode_staticcall(backend),

                0xFD => self.opcode_revert(backend),
                0xFE => self.opcode_invalid(backend),

                0xFF => self.opcode_selfdestruct(backend),
                _ => self.opcode_unknown(backend),
            } {
                Ok(result) => result,
                Err(e) => {
                    let message = build_revert_message(&e.to_string());
                    self.opcode_revert_impl(Buffer::from_slice(&message), backend)?
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
        };

        tracing_event!(
            self,
            tracing::Event::EndVM {
                status: status.clone()
            }
        );

        Ok((status, step))
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
            stack: Stack::new(
                #[cfg(feature = "tracing")]
                self.tracer.clone(),
            ),
            memory: Memory::new(
                #[cfg(feature = "tracing")]
                self.tracer.clone(),
            ),
            pc: 0_usize,
            is_static: self.is_static,
            reason,
            parent: None,
            phantom: PhantomData,
            #[cfg(feature = "tracing")]
            tracer: self.tracer.clone(),
        };

        core::mem::swap(self, &mut other);
        self.parent = Some(Box::new(other));
    }

    fn join(&mut self) -> Self {
        assert!(self.parent.is_some());

        let mut other = *self.parent.take().unwrap();
        core::mem::swap(self, &mut other);

        other
    }
}
