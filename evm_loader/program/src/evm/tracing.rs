use maybe_async::maybe_async;

use super::{Context, ExitStatus};
use crate::evm::database::Database;
use crate::evm::opcode_table::Opcode;

#[maybe_async(?Send)]
pub trait EventListener {
    async fn event(
        &mut self,
        executor_state: &impl Database,
        event: Event,
    ) -> crate::error::Result<()>;
}

#[maybe_async(?Send)]
impl EventListener for () {
    async fn event(
        &mut self,
        _executor_state: &impl Database,
        _event: Event,
    ) -> crate::error::Result<()> {
        Ok(())
    }
}

/// Trace event
pub enum Event {
    BeginVM {
        context: Context,
        chain_id: u64,
        input: Vec<u8>,
        opcode: Opcode,
    },
    EndVM {
        context: Context,
        chain_id: u64,
        status: ExitStatus,
    },
    BeginStep {
        context: Context,
        chain_id: u64,
        opcode: Opcode,
        pc: usize,
        stack: Vec<[u8; 32]>,
        memory: Vec<u8>,
        return_data: Vec<u8>,
    },
}

macro_rules! tracing_event {
    ($self:expr, $backend:expr, $event:expr) => {
        #[cfg(not(target_os = "solana"))]
        if let Some(tracer) = &mut $self.tracer {
            tracer.event($backend, $event).await?;
        }
    };
}

macro_rules! begin_vm {
    ($self:expr, $backend:expr, $context:expr, $chain_id:expr, $input:expr, $opcode:expr) => {
        $crate::evm::tracing::tracing_event!(
            $self,
            $backend,
            crate::evm::tracing::Event::BeginVM {
                context: $context,
                chain_id: $chain_id,
                input: $input.to_vec(),
                opcode: $opcode
            }
        );
    };
    ($self:expr, $backend:expr, $context:expr, $chain_id:expr, $input:expr) => {
        $crate::evm::tracing::begin_vm!(
            $self,
            $backend,
            $context,
            $chain_id,
            $input,
            $self.execution_code.get_u8($self.pc, &$self.parent).into()
        );
    };
}

macro_rules! begin_vm_inner {
    ($self:expr, $backend:expr, $context:expr, $chain_id:expr, $input_offset:expr, $input_len:expr) => {
        $crate::evm::tracing::begin_vm!(
            $self,
            $backend,
            $context,
            $chain_id,
            $self.memory.slice($input_offset, $input_len),
            $self.execution_code.get_u8($self.pc, &$self.parent).into()
        );
    };
}

macro_rules! stop_vm {
    ($self:expr, $backend:expr, $status:expr) => {
        $crate::evm::tracing::tracing_event!(
            $self,
            $backend,
            crate::evm::tracing::Event::EndVM {
                context: $self.context,
                chain_id: $self.chain_id,
                status: $status
            }
        );
    };
}

macro_rules! return_vm {
    ($self:expr, $backend:expr, $status:expr) => {
        $crate::evm::tracing::tracing_event!(
            $self,
            $backend,
            crate::evm::tracing::Event::EndVM {
                context: $self.context,
                chain_id: $self.chain_id,
                status: $status($crate::types::vector::VectorSliceExt::to_vector(
                    $self
                        .memory
                        .slice($self.return_data.start, $self.return_data.len()),
                    $crate::allocator::acc_allocator()
                ))
            }
        );
    };
}

macro_rules! begin_step {
    ($self:expr, $backend:expr) => {
        $crate::evm::tracing::tracing_event!(
            $self,
            $backend,
            crate::evm::tracing::Event::BeginStep {
                context: $self.context,
                chain_id: $self.chain_id,
                opcode: $self.execution_code.get_u8($self.pc, &$self.parent).into(),
                pc: $self.pc,
                stack: $self.stack.to_vec(),
                memory: $self.memory.to_vec(),
                return_data: $self
                    .child
                    .as_ref()
                    .map_or(vec![], |c| c.return_data().to_vec())
            }
        );
    };
}

pub(crate) use begin_step;
pub(crate) use begin_vm;
pub(crate) use begin_vm_inner;
pub(crate) use return_vm;
pub(crate) use stop_vm;
pub(crate) use tracing_event;
