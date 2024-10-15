use serde::{Deserialize, Serialize};

#[allow(clippy::struct_excessive_bools)]
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub block: u64,
    pub index: Option<u64>,
    pub is_reset: bool,
    pub is_return: bool,
    pub is_cancel: bool,
    pub is_no_chain_id: bool,
    pub steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMap {
    pub steps: Vec<ExecutionStep>,
}

impl ExecutionMap {
    #[must_use]
    pub fn has_step_no_chain_id(&self) -> bool {
        self.steps.iter().any(|s| s.is_no_chain_id)
    }
}
