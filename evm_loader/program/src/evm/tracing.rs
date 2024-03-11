use maybe_async::maybe_async;

use super::{Context, ExitStatus};
use crate::evm::database::Database;
use crate::evm::opcode_table::Opcode;

pub struct NoopEventListener;

#[maybe_async(?Send)]
pub trait EventListener {
    async fn event(
        &mut self,
        executor_state: &impl Database,
        event: Event,
    ) -> crate::error::Result<()>;
}

#[maybe_async(?Send)]
impl EventListener for NoopEventListener {
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
