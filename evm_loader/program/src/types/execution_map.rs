use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub block: u64,
    pub index: Option<u64>,
    pub is_reset: bool,
    pub is_return: bool,
    pub steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMap {
    pub steps: Vec<ExecutionStep>,
}

impl ExecutionMap {
    #[must_use]
    pub fn has_reset(&self) -> bool {
        self.steps.iter().any(|s| s.is_reset)
    }
}
