#![allow(unused_mut)]

use allocator_api2::alloc::{self, Allocator};
use allocator_api2::boxed::Box;
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::instruction::Instruction;
use std::{fmt::Display, ops::Range};

use crate::debug::log_data;
use crate::error::{build_revert_message, Error, Result};
use crate::evm::opcode::Action;
use crate::evm::tracing::EventListener;
use crate::evm::utils::Buffer;
use crate::types::{Address, Transaction};

use self::{database::Database, memory::Memory, stack::Stack};

pub mod database;
mod memory;
pub mod opcode;
pub mod opcode_table;
pub mod precompile;
mod stack;
pub mod tracing;
mod utils;

pub type SolanaCallInterrupt = std::boxed::Box<(Instruction, Vec<Vec<u8>>, Option<u64>)>;

#[derive(Debug, Clone, Eq, PartialEq)]
#[repr(C)]
pub enum ExitStatus {
    Stop,
    Return(Vec<u8>),
    Revert(Vec<u8>),
    Suicide,
    Interrupted(SolanaCallInterrupt),
    StepLimit,
    Cancel,
}

impl Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.status())
    }
}

impl ExitStatus {
    #[must_use]
    pub fn status(&self) -> &'static str {
        match self {
            ExitStatus::Return(_) | ExitStatus::Stop | ExitStatus::Suicide => "succeed",
            ExitStatus::Revert(_) => "revert",
            ExitStatus::Interrupted(_) => "interrupted due Solana call",
            ExitStatus::StepLimit => "step limit exceeded",
            ExitStatus::Cancel => "cancel",
        }
    }

    #[must_use]
    pub fn code(&self) -> u8 {
        // No idea where these numbers come from, they existed from the very beginning
        // Keeping for backward compatibility
        match self {
            ExitStatus::Stop => 0x11,
            ExitStatus::Return(_) => 0x12,
            ExitStatus::Revert(_) => 0xd0,
            ExitStatus::Suicide => 0x13,
            ExitStatus::Interrupted(_) | ExitStatus::StepLimit | ExitStatus::Cancel => 0xFF,
        }
    }

    #[must_use]
    pub fn is_execution_finished(&self) -> bool {
        matches!(
            self,
            ExitStatus::Stop | ExitStatus::Return(_) | ExitStatus::Revert(_) | ExitStatus::Suicide
        )
    }

    #[must_use]
    pub fn is_succeed(&self) -> Option<bool> {
        match self {
            ExitStatus::Stop | ExitStatus::Return(_) | ExitStatus::Suicide => Some(true),
            ExitStatus::Revert(_) | ExitStatus::Cancel => Some(false),
            ExitStatus::Interrupted(_) | ExitStatus::StepLimit => None,
        }
    }

    #[must_use]
    pub fn into_result(self) -> Option<Vec<u8>> {
        match self {
            ExitStatus::Return(v) | ExitStatus::Revert(v) => Some(v),
            ExitStatus::Stop
            | ExitStatus::Suicide
            | ExitStatus::Interrupted(_)
            | ExitStatus::StepLimit
            | ExitStatus::Cancel => None,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[repr(C)]
pub enum Reason {
    Call,
    Create,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Context {
    pub caller: Address,
    pub contract: Address,
    pub contract_chain_id: u64,
    pub value: U256,
    pub code_address: Option<Address>,
}

#[repr(C)]
pub struct Machine<A, T = ()>
where
    A: Allocator + Copy,
    T: EventListener,
{
    origin: Address,
    chain_id: u64,
    context: Context,

    gas_price: U256,
    gas_limit: U256,

    execution_code: Buffer<A>,
    call_data: Buffer<A>,
    return_data: Range<usize>,
    child_return_into: Range<usize>,

    stack: Stack<A>,
    memory: Memory<A>,
    pc: usize,

    is_static: bool,
    reason: Reason,

    parent: Option<Box<Self, A>>,
    child: Option<Box<Self, A>>,

    allocator: A,
    tracer: Option<T>,
}

impl<A> Machine<A, ()>
where
    A: Allocator + Copy,
{
    #[maybe_async]
    pub async fn new_in(
        trx: &dyn Transaction,
        origin: Address,
        backend: &mut impl Database,
        allocator: A,
    ) -> Result<Self> {
        Self::with_tracer_in(trx, origin, backend, None, allocator).await
    }
}

impl<T> Machine<alloc::Global, T>
where
    T: EventListener,
{
    #[maybe_async]
    pub async fn with_tracer(
        trx: &dyn Transaction,
        origin: Address,
        backend: &mut impl Database,
        tracer: Option<T>,
    ) -> Result<Self> {
        Self::with_tracer_in(trx, origin, backend, tracer, alloc::Global).await
    }
}

impl<A, T> Machine<A, T>
where
    A: Allocator + Copy,
    T: EventListener,
{
    #[maybe_async]
    pub async fn with_tracer_in(
        trx: &dyn Transaction,
        origin: Address,
        backend: &mut impl Database,
        tracer: Option<T>,
        allocator: A,
    ) -> Result<Self> {
        let chain_id = trx.chain_id().unwrap_or_else(|| backend.default_chain_id());

        if backend.balance(origin, chain_id).await? < trx.value() {
            return Err(Error::InsufficientBalance(origin, chain_id, trx.value()));
        }

        if trx.target().is_some() {
            Self::new_call(chain_id, trx, origin, backend, tracer, allocator).await
        } else {
            Self::new_create(chain_id, trx, origin, backend, tracer, allocator).await
        }
    }

    #[maybe_async]
    async fn new_call(
        chain_id: u64,
        trx: &dyn Transaction,
        origin: Address,
        backend: &mut impl Database,
        tracer: Option<T>,
        allocator: A,
    ) -> Result<Self> {
        assert!(trx.target().is_some());

        let target = *trx.target().unwrap();
        log_data(&[b"ENTER", b"CALL", target.as_bytes()]);

        backend.snapshot();

        backend
            .transfer(origin, target, chain_id, trx.value())
            .await?;

        let execution_code = backend.code(target, allocator).await?;
        let mut machine = Self {
            origin,
            chain_id,
            context: Context {
                caller: origin,
                contract: target,
                contract_chain_id: backend.contract_chain_id(target).await.unwrap_or(chain_id),
                value: trx.value(),
                code_address: Some(target),
            },
            gas_price: trx.gas_price(),
            gas_limit: trx.gas_limit(),
            execution_code: Buffer::from_vec(execution_code),
            call_data: Buffer::from_slice_in(trx.call_data(), allocator),
            return_data: 0..0,
            child_return_into: 0..0,
            stack: Stack::new_in(allocator),
            memory: Memory::new_in(allocator),
            pc: 0_usize,
            is_static: false,
            reason: Reason::Call,
            parent: None,
            child: None,
            allocator,
            tracer,
        };

        tracing::begin_vm!(
            machine,
            backend,
            machine.context,
            machine.chain_id,
            trx.call_data(),
            opcode_table::CALL
        );

        Ok(machine)
    }

    #[maybe_async]
    async fn new_create(
        chain_id: u64,
        trx: &dyn Transaction,
        origin: Address,
        backend: &mut impl Database,
        tracer: Option<T>,
        allocator: A,
    ) -> Result<Self> {
        assert!(trx.target().is_none());

        let target = Address::from_create(&origin, trx.nonce());
        log_data(&[b"ENTER", b"CREATE", target.as_bytes()]);

        if (backend.nonce(target, chain_id).await? != 0) || (backend.code_size(target).await? != 0)
        {
            return Err(Error::DeployToExistingAccount(target, origin));
        }

        backend.snapshot();

        backend.start_create(target, chain_id).await?;
        backend.increment_nonce(target, chain_id).await?;
        backend
            .transfer(origin, target, chain_id, trx.value())
            .await?;

        let mut machine = Self {
            origin,
            chain_id,
            context: Context {
                caller: origin,
                contract: target,
                contract_chain_id: chain_id,
                value: trx.value(),
                code_address: None,
            },
            gas_price: trx.gas_price(),
            gas_limit: trx.gas_limit(),
            return_data: 0..0,
            child_return_into: 0..0,
            stack: Stack::new_in(allocator),
            memory: Memory::new_in(allocator),
            pc: 0_usize,
            is_static: false,
            reason: Reason::Create,
            execution_code: Buffer::from_slice_in(trx.call_data(), allocator),
            call_data: Buffer::from_slice_in(&[], allocator),
            parent: None,
            child: None,
            allocator,
            tracer,
        };
        tracing::begin_vm!(
            machine,
            backend,
            machine.context,
            machine.chain_id,
            trx.call_data(),
            opcode_table::CREATE
        );

        Ok(machine)
    }

    #[maybe_async]
    pub async fn execute(
        &mut self,
        step_limit: u64,
        backend: &mut impl Database,
    ) -> Result<(ExitStatus, u64)> {
        let contract = self.context.contract;
        let (status, step) = match self.try_call_precompile(&contract, backend).await {
            Some(Ok(value)) => {
                self.return_from_stack_frame(&value, backend).await?;
                let return_data = self.return_data().to_vec();
                (ExitStatus::Return(return_data), 0)
            }
            Some(Err(error)) => {
                self.revert_from_stack_frame(error, backend).await?;
                let revert_data = self.return_data().to_vec();
                (ExitStatus::Revert(revert_data), 0)
            }
            None => self.run_loop(step_limit, backend).await?,
        };

        Ok((status, step))
    }

    #[maybe_async]
    async fn run_loop(
        &mut self,
        step_limit: u64,
        backend: &mut impl Database,
    ) -> Result<(ExitStatus, u64)> {
        let mut step = 0_u64;

        let status = loop {
            if step >= step_limit {
                break ExitStatus::StepLimit;
            }
            step += 1;

            let opcode = self.execution_code.get_u8(self.pc, &self.parent);

            tracing::begin_step!(self, backend);

            let opcode_result = match self.execute_opcode(backend, opcode).await {
                Ok(result) => result,
                Err(error) => self.revert_from_stack_frame(error, backend).await?,
            };

            match opcode_result {
                Action::Continue => self.pc += 1,
                Action::Jump(target) => self.pc = target,
                Action::Stop => break ExitStatus::Stop,
                Action::Return => {
                    let return_data = self.return_data().to_vec();
                    break ExitStatus::Return(return_data);
                }
                Action::Revert => {
                    let return_data = self.return_data().to_vec();
                    break ExitStatus::Revert(return_data);
                }
                Action::Suicide => break ExitStatus::Suicide,
                Action::Interrupted(state) => {
                    break ExitStatus::Interrupted(state);
                }
                Action::Noop => {}
            };
        };

        Ok((status, step))
    }

    fn new_child(
        &self,
        reason: Reason,
        chain_id: u64,
        context: Context,
        execution_code: Buffer<A>,
        call_data: Buffer<A>,
        gas_limit: U256,
    ) -> Box<Self, A> {
        let allocator = self.allocator;

        let machine = Self {
            origin: self.origin,
            chain_id,
            context,
            gas_price: self.gas_price,
            gas_limit,
            execution_code,
            call_data,
            return_data: 0..0,
            child_return_into: 0..0,
            stack: Stack::new_in(allocator),
            memory: Memory::new_in(allocator),
            pc: 0_usize,
            is_static: self.is_static,
            reason,
            parent: None,
            child: None,
            tracer: None,
            allocator,
        };
        Box::new_in(machine, allocator)
    }

    #[allow(clippy::too_many_arguments)]
    fn reset_child(
        &self,
        mut child: Box<Self, A>,
        reason: Reason,
        chain_id: u64,
        context: Context,
        execution_code: Buffer<A>,
        call_data: Buffer<A>,
        gas_limit: U256,
    ) -> Box<Self, A> {
        child.reason = reason;
        child.chain_id = chain_id;
        child.context = context;
        child.execution_code = execution_code;
        child.call_data = call_data;
        child.gas_limit = gas_limit;
        child.is_static = self.is_static;
        child.return_data = 0..0;
        child.pc = 0;

        child.memory.reset();
        child.stack.reset();

        if let Some(grandchild) = child.child.as_mut() {
            grandchild.return_data = 0..0;
        }

        child
    }

    fn fork(
        &mut self,
        reason: Reason,
        chain_id: u64,
        context: Context,
        execution_code: Buffer<A>,
        call_data: Buffer<A>,
        gas_limit: Option<U256>,
    ) {
        let gas_limit = gas_limit.unwrap_or(self.gas_limit);

        #[rustfmt::skip]
        let mut other = if let Some(mut child) = self.child.take() {
            // Reuse the existing child, so we don't need to allocate stack and memory again.
            self.reset_child(child, reason, chain_id, context, execution_code, call_data, gas_limit)
        } else {
            self.new_child(reason, chain_id, context, execution_code, call_data, gas_limit)
        };

        let tracer = self.take_tracer();
        other.set_tracer(tracer);

        core::mem::swap(self, &mut other);
        self.parent = Some(other);
    }

    fn join(&mut self) {
        assert!(self.parent.is_some());

        let mut other = self.parent.take().unwrap();
        core::mem::swap(self, other.as_mut());

        self.tracer = other.tracer.take();
        self.child = Some(other);
    }

    pub fn return_data(&self) -> &[u8] {
        let offset = self.return_data.start;
        let length = self.return_data.len();

        self.memory.slice(offset, length)
    }

    #[maybe_async]
    pub async fn return_from_stack_frame(
        &mut self,
        return_data: &[u8],
        backend: &mut impl Database,
    ) -> Result<Action> {
        self.memory.write(0, return_data)?;

        self.stack.push_usize(return_data.len())?;
        self.stack.push_zero()?; // offset

        self.opcode_return(backend).await
    }

    #[maybe_async]
    pub async fn revert_from_stack_frame(
        &mut self,
        error: impl std::error::Error,
        backend: &mut impl Database,
    ) -> Result<Action> {
        let message = build_revert_message(&error.to_string());

        self.memory.write(0, &message)?;

        self.stack.push_usize(message.len())?;
        self.stack.push_zero()?; // offset

        self.opcode_revert(backend).await
    }

    pub fn set_tracer(&mut self, tracer: Option<T>) {
        self.tracer = tracer;
    }

    pub fn into_tracer(self) -> Option<T> {
        self.tracer
    }

    pub fn take_tracer(&mut self) -> Option<T> {
        self.tracer.take()
    }

    pub fn increment_pc(&mut self) {
        self.pc += 1;
    }

    pub fn context(&self) -> &Context {
        &self.context
    }
}
