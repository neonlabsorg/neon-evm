use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub block: u64,
    pub index: Option<u64>,
    pub is_reset: bool,
    pub is_cancel: bool,
    pub is_return: bool,
    pub steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMap {
    pub steps: Vec<ExecutionStep>,
    pub is_skd_transaction: bool,
}
