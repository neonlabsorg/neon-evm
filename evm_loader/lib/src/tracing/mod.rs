use ethnum::U256;
use serde_json::Value;
use serde_with::serde_as;
use web3::types::{Bytes, H256};
pub mod tracers;
use crate::tracing::de::Deserialize;
use evm_loader::types::Address;
use serde::de::{self, Deserializer};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountOverride {
    pub nonce: Option<u64>,
    pub code: Option<Bytes>,
    pub balance: Option<U256>,
    pub state: Option<HashMap<H256, H256>>,
    pub state_diff: Option<HashMap<H256, H256>>,
}

impl AccountOverride {
    #[must_use]
    pub fn storage(&self, index: U256) -> Option<[u8; 32]> {
        match (&self.state, &self.state_diff) {
            (None, None) => None,
            (Some(_), Some(_)) => {
                panic!("Account has both `state` and `stateDiff` overrides")
            }
            (Some(state), None) => {
                return state
                    .get(&H256::from(index.to_be_bytes()))
                    .map(|value| value.to_fixed_bytes())
            }
            (None, Some(state_diff)) => state_diff
                .get(&H256::from(index.to_be_bytes()))
                .map(|v| v.to_fixed_bytes()),
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
    #[serde(default)]
    pub limit: usize,
    pub tracer: Option<String>,
    pub timeout: Option<String>,
    pub tracer_config: Option<Value>,
}

/// We have complex key as address@chain_id from requests.
#[derive(Eq, PartialEq, Hash, Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
#[serde_as]
pub struct ChainBalanceOverrideKey {
    pub address: Address,
    pub chain_id: u64,
}

impl<'de> Deserialize<'de> for ChainBalanceOverrideKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ChainBalanceOverrideKey;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string in the format \"address@chain_id\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<ChainBalanceOverrideKey, E>
            where
                E: serde::de::Error,
            {
                let parts: Vec<&str> = value.split('@').collect();
                if parts.len() != 2 {
                    return Err(E::custom(format!("invalid format: {value}")));
                }

                let address = Address::from_str(parts[0]).map_err(E::custom)?;
                let chain_id = u64::from_str(parts[1]).map_err(E::custom)?;

                Ok(ChainBalanceOverrideKey { address, chain_id })
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[serde_as]
pub struct ChainBalanceOverride {
    pub nonce: Option<u64>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub balance: Option<U256>,
}

pub type ChainBalanceOverrides = HashMap<ChainBalanceOverrideKey, ChainBalanceOverride>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/api.go#L163>
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::module_name_repetitions)]
pub struct TraceCallConfig {
    #[serde(flatten)]
    pub trace_config: TraceConfig,
    pub block_overrides: Option<BlockOverrides>,
    pub state_overrides: Option<AccountOverrides>,
    #[serde(default)]
    pub balance_overrides: Option<ChainBalanceOverrides>,
}

#[cfg(test)]
#[path = "./mod_tests.rs"]
mod mod_tests;
