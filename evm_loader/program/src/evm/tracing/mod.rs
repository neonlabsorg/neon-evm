use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use crate::executor::Action;
use ethnum::U256;
use serde_json::Value;

use super::{Context, ExitStatus};

#[allow(clippy::all)]
pub mod trace;
pub mod tracers;

#[derive(Debug, Clone)]
pub struct EmulationResult {
    pub exit_status: ExitStatus,
    pub steps_executed: u64,
    pub used_gas: u64,
    pub actions: Vec<Action>,
}

pub trait EventListener: Send + Sync + Debug {
    fn event(&mut self, event: Event);
    fn into_traces(self: Box<Self>, emulation_result: EmulationResult) -> Value;
}

pub type TracerType = Arc<RwLock<Box<dyn EventListener>>>;
pub type TracerTypeOpt = Option<TracerType>;

/// Trace event
pub enum Event {
    BeginVM {
        context: Context,
        code: Vec<u8>,
    },
    EndVM {
        status: ExitStatus,
    },
    BeginStep {
        opcode: u8,
        pc: usize,
        stack: Vec<[u8; 32]>,
        memory: Vec<u8>,
    },
    EndStep {
        gas_used: u64,
        return_data_getter: Option<Box<dyn Fn() -> Vec<u8>>>,
    },
    StackPush {
        value: [u8; 32],
    },
    MemorySet {
        offset: usize,
        data: Vec<u8>,
    },
    StorageSet {
        index: U256,
        value: [u8; 32],
    },
    StorageAccess {
        index: U256,
        value: [u8; 32],
    },
}
