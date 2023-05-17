use crate::types::trace::{TraceCallConfig, TraceConfig};
use evm_loader::types::Address;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct GetEtherRequest {
    pub ether: Option<String>,
    pub slot: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct GetStorageAtRequest {
    pub contract_id: String,
    pub index: Option<String>,
    pub slot: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct TxParamsRequestModel {
    pub sender: Address,
    pub contract: Option<String>,
    pub data: Option<Vec<u8>>,
    pub value: Option<String>,
    pub gas_limit: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct EmulationParamsRequestModel {
    pub token_mint: Option<String>,
    pub chain_id: Option<u64>,
    pub max_steps_to_execute: u64,
    pub cached_accounts: Option<Vec<Address>>,
    pub solana_accounts: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct EmulateRequestModel {
    #[serde(flatten)]
    pub tx_params: TxParamsRequestModel,
    #[serde(flatten)]
    pub emulation_params: EmulationParamsRequestModel,
    pub slot: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct EmulateHashRequestModel {
    #[serde(flatten)]
    pub tx_params: TxParamsRequestModel,
    #[serde(flatten)]
    pub emulation_params: EmulationParamsRequestModel,
    pub hash: String,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct TraceRequestModel {
    #[serde(flatten)]
    pub emulate_request: EmulateRequestModel,
    pub trace_call_config: Option<TraceCallConfig>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct TraceHashRequestModel {
    #[serde(flatten)]
    pub emulate_hash_request: EmulateHashRequestModel,
    pub trace_config: Option<TraceConfig>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct TraceNextBlockParamsRequest {
    #[serde(flatten)]
    pub emulation_params: EmulationParamsRequestModel,
    pub slot: u64,
    pub trace_config: Option<TraceConfig>,
}
