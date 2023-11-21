use crate::types::Address;
use ethnum::U256;
use evm_loader::account::ether_account;
use evm_loader::types::hexbytes::HexBytes;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;

#[cfg(test)]
pub mod tests;
pub mod tracers;

/// See <https://github.com/ethereum/go-ethereum/blob/master/internal/ethapi/api.go#L993>
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockOverrides {
    pub number: Option<u64>,
    #[allow(unused)]
    pub difficulty: Option<U256>, // NOT SUPPORTED by Neon EVM
    pub time: Option<i64>,
    #[allow(unused)]
    pub gas_limit: Option<u64>, // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub coinbase: Option<Address>, // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub random: Option<U256>, // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub base_fee: Option<U256>, // NOT SUPPORTED BY Neon EVM
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/internal/ethapi/api.go#L942>
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOverride {
    pub nonce: Option<u64>,
    pub code: Option<HexBytes>,
    pub balance: Option<U256>,
    pub state: Option<HashMap<U256, U256>>,
    pub state_diff: Option<HashMap<U256, U256>>,
}

impl AccountOverride {
    pub fn apply(&self, ether_account: &mut ether_account::Data) {
        if let Some(nonce) = self.nonce {
            ether_account.trx_count = nonce;
        }
        if let Some(balance) = self.balance {
            ether_account.balance = balance;
        }
        #[allow(clippy::cast_possible_truncation)]
        if let Some(code) = &self.code {
            ether_account.code_size = code.len() as u32;
        }
    }
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/internal/ethapi/api.go#L951>
pub type AccountOverrides = HashMap<Address, AccountOverride>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/api.go#L151>
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::module_name_repetitions, clippy::struct_excessive_bools)]
pub struct TraceConfig {
    #[serde(default)]
    pub enable_memory: bool,
    #[serde(default)]
    pub disable_storage: bool,
    #[serde(default)]
    pub disable_stack: bool,
    #[serde(default)]
    pub enable_return_data: bool,
    pub tracer: Option<String>,
    pub timeout: Option<String>,
    pub tracer_config: Option<Value>,
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/api.go#L163>
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::module_name_repetitions)]
pub struct TraceCallConfig {
    #[serde(flatten)]
    pub trace_config: TraceConfig,
    pub block_overrides: Option<BlockOverrides>,
    pub state_overrides: Option<AccountOverrides>,
}

impl From<TraceConfig> for TraceCallConfig {
    fn from(trace_config: TraceConfig) -> Self {
        Self {
            trace_config,
            ..Self::default()
        }
    }
}
