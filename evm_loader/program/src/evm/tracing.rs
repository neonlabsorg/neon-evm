use maybe_async::maybe_async;

use super::{Context, ExitStatus, Reason};
use crate::evm::database::Database;

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
        code: Vec<u8>,
        reason: Reason,
    },
    EndVM {
        context: Context,
        chain_id: u64,
        status: ExitStatus,
    },
    BeginStep {
        context: Context,
        chain_id: u64,
        opcode: u8,
        pc: usize,
        stack: Vec<[u8; 32]>,
        memory: Vec<u8>,
    },
    EndStep {
        return_data: Option<Vec<u8>>,
    },
}
