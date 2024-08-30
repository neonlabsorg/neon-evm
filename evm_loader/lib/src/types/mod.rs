pub mod tracer_ch_common;

pub mod no_db;
pub(crate) mod tracer_ch_db;
pub mod tracer_rocks_db;

use crate::commands::get_config::ChainInfo;
use crate::config::DbConfig;
use crate::tracing::TraceCallConfig;
pub use crate::types::no_db::NoDb;
use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};
pub use crate::types::tracer_ch_db::ClickHouseDb;
pub use crate::types::tracer_rocks_db::RocksDb;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use ethnum::U256;
use evm_loader::solana_program::clock::{Slot, UnixTimestamp};
pub use evm_loader::types::Address;
use evm_loader::types::{StorageKey, Transaction};
use evm_loader::{
    account_storage::AccountStorage,
    types::{
        vector::VectorVecExt, vector::VectorVecSlowExt, AccessListTx, DynamicFeeTx, LegacyTx,
        TransactionPayload,
    },
};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use serde_with::{hex::Hex, serde_as, DisplayFromStr, OneOrMany};
use solana_sdk::signature::Signature;
use solana_sdk::{account::Account, pubkey::Pubkey};
use std::collections::HashMap;
use DbConfig::{ChDbConfig, NoDbConfig, RocksDbConfig};

pub type DbResult<T> = Result<T, anyhow::Error>;

#[enum_dispatch]
pub enum TracerDbType {
    ClickHouseDb,
    RocksDb,
    NoDb,
}

impl TracerDbType {
    pub async fn from_config(db_config: &DbConfig) -> Self {
        match db_config {
            RocksDbConfig(rocks_db_config) => RocksDb::new(rocks_db_config).await.into(),
            ChDbConfig(ch_db_config) => ClickHouseDb::new(ch_db_config).into(),
            NoDbConfig(..) => NoDb::default().into(),
        }
    }
}

impl Clone for TracerDbType {
    fn clone(&self) -> Self {
        match self {
            Self::RocksDb(r) => r.clone().into(),
            Self::ClickHouseDb(c) => c.clone().into(),
            Self::NoDb(n) => n.clone().into(),
        }
    }
}

#[async_trait]
#[enum_dispatch(TracerDbType)]
pub trait TracerDb {
    async fn get_block_time(&self, slot: Slot) -> DbResult<UnixTimestamp>;

    async fn get_earliest_rooted_slot(&self) -> DbResult<u64>;

    async fn get_latest_block(&self) -> DbResult<u64>;

    async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
    ) -> DbResult<Option<Account>>;

    async fn get_transaction_index(&self, signature: Signature) -> DbResult<u64>;

    async fn get_neon_revisions(&self, _pubkey: &Pubkey) -> DbResult<RevisionMap>;

    async fn get_neon_revision(&self, _slot: Slot, _pubkey: &Pubkey) -> DbResult<String>;

    async fn get_slot_by_blockhash(&self, blockhash: String) -> DbResult<u64>;

    async fn get_sync_status(&self) -> DbResult<EthSyncStatus>;
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AccessListItem {
    pub address: Address,
    #[serde(rename = "storageKeys")]
    #[serde_as(as = "Vec<Hex>")]
    pub storage_keys: Vec<StorageKey>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct TxParams {
    pub nonce: Option<u64>,
    pub from: Address,
    pub to: Option<Address>,
    #[serde_as(as = "Option<Hex>")]
    pub data: Option<Vec<u8>>,
    pub value: Option<U256>,
    pub gas_limit: Option<U256>,
    pub actual_gas_used: Option<U256>,
    pub gas_price: Option<U256>,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
    pub access_list: Option<Vec<AccessListItem>>,
    pub chain_id: Option<u64>,
}

impl TxParams {
    pub async fn into_transaction(self, backend: &impl AccountStorage) -> (Address, Transaction) {
        let chain_id = self.chain_id.unwrap_or_else(|| backend.default_chain_id());

        let origin_nonce = backend.nonce(self.from, chain_id).await;
        let nonce = self.nonce.unwrap_or(origin_nonce);

        let payload = if let Some(access_list) = self.access_list {
            let access_list: Vec<_> = access_list
                .into_iter()
                .map(|a| (a.address, a.storage_keys.into_vector()))
                .collect();

            if let Some(max_priority_fee_per_gas) = self.max_priority_fee_per_gas {
                let dynamic_fee_tx = DynamicFeeTx {
                    nonce,
                    max_priority_fee_per_gas,
                    max_fee_per_gas: self.max_fee_per_gas.unwrap_or(max_priority_fee_per_gas * 2),
                    gas_limit: self.gas_limit.unwrap_or(U256::MAX),
                    target: self.to,
                    value: self.value.unwrap_or_default(),
                    call_data: self.data.unwrap_or_default().into_vector(),
                    chain_id: U256::from(chain_id),
                    access_list: access_list.elementwise_copy_into_vector(),
                    r: U256::ZERO,
                    s: U256::ZERO,
                    recovery_id: 0,
                };
                TransactionPayload::DynamicFee(dynamic_fee_tx)
            } else {
                let access_list_tx = AccessListTx {
                    nonce,
                    gas_price: U256::ZERO,
                    gas_limit: self.gas_limit.unwrap_or(U256::MAX),
                    target: self.to,
                    value: self.value.unwrap_or_default(),
                    call_data: self.data.unwrap_or_default().into_vector(),
                    chain_id: U256::from(chain_id),
                    access_list: access_list.elementwise_copy_into_vector(),
                    r: U256::ZERO,
                    s: U256::ZERO,
                    recovery_id: 0,
                };
                TransactionPayload::AccessList(access_list_tx)
            }
        } else {
            let legacy_tx = LegacyTx {
                nonce,
                gas_price: U256::ZERO,
                gas_limit: self.gas_limit.unwrap_or(U256::MAX),
                target: self.to,
                value: self.value.unwrap_or_default(),
                call_data: self.data.unwrap_or_default().into_vector(),
                chain_id: self.chain_id.map(U256::from),
                v: U256::ZERO,
                r: U256::ZERO,
                s: U256::ZERO,
                recovery_id: 0,
            };
            TransactionPayload::Legacy(legacy_tx)
        };

        let tx = Transaction {
            transaction: payload,
            byte_len: 0,
            hash: [0; 32],
            signed_hash: [0; 32],
        };

        (self.from, tx)
    }

    #[must_use]
    pub fn from_transaction(origin: Address, tx: &Transaction) -> Self {
        Self {
            from: origin,
            to: tx.target(),
            nonce: Some(tx.nonce()),
            data: Some(tx.call_data().to_vec()),
            value: Some(tx.value()),
            gas_limit: Some(tx.gas_limit()),
            gas_price: Some(tx.gas_price()),
            max_fee_per_gas: tx.max_fee_per_gas(),
            max_priority_fee_per_gas: tx.max_priority_fee_per_gas(),
            chain_id: tx.chain_id(),
            access_list: None,
            actual_gas_used: None,
        }
    }
}

impl std::fmt::Debug for TxParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let json = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;

        f.write_str(&json)
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SerializedAccount {
    pub lamports: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub owner: Pubkey,
    pub executable: bool,
    pub rent_epoch: u64,
    #[serde_as(as = "Hex")]
    pub data: Vec<u8>,
}

impl From<&SerializedAccount> for Account {
    fn from(account: &SerializedAccount) -> Self {
        Self {
            lamports: account.lamports,
            owner: account.owner,
            executable: account.executable,
            rent_epoch: account.rent_epoch,
            data: account.data.clone(),
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountInfoLevel {
    Changed,
    All,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateRequest {
    pub tx: TxParams,
    pub step_limit: Option<u64>,
    pub chains: Option<Vec<ChainInfo>>,
    pub trace_config: Option<TraceCallConfig>,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub accounts: Vec<Pubkey>,
    #[serde_as(as = "Option<HashMap<DisplayFromStr,_>>")]
    pub solana_overrides: Option<HashMap<Pubkey, Option<SerializedAccount>>>,
    pub provide_account_info: Option<AccountInfoLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateApiRequest {
    #[serde(flatten)]
    pub body: EmulateRequest,
    pub slot: Option<u64>,
    pub tx_index_in_block: Option<u64>,
    pub id: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct BalanceAddress {
    pub address: Address,
    pub chain_id: u64,
}

impl BalanceAddress {
    #[must_use]
    pub fn find_pubkey(&self, program_id: &Pubkey) -> Pubkey {
        self.address
            .find_balance_address(program_id, self.chain_id)
            .0
    }

    #[must_use]
    pub fn find_contract_pubkey(&self, program_id: &Pubkey) -> Pubkey {
        self.address.find_solana_address(program_id).0
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct GetBalanceRequest {
    #[serde_as(as = "OneOrMany<_>")]
    pub account: Vec<BalanceAddress>,
    pub slot: Option<u64>,
    pub id: Option<String>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct GetContractRequest {
    #[serde_as(as = "OneOrMany<_>")]
    pub contract: Vec<Address>,
    pub slot: Option<u64>,
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct GetStorageAtRequest {
    pub contract: Address,
    pub index: U256,
    pub slot: Option<u64>,
    pub id: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct CancelTrxRequest {
    pub storage_account: Pubkey,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct RequestWithSlot {
    pub slot: Option<u64>,
    pub tx_index_in_block: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct GetNeonElfRequest {
    pub program_location: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct InitEnvironmentRequest {
    pub send_trx: bool,
    pub force: bool,
    pub keys_dir: Option<String>,
    pub file: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct GetHolderRequest {
    #[serde_as(as = "DisplayFromStr")]
    pub pubkey: Pubkey,
    pub slot: Option<u64>,
    pub id: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct SimulateSolanaRequest {
    pub compute_units: Option<u64>,
    pub heap_size: Option<u32>,
    pub account_limit: Option<usize>,
    pub verify: Option<bool>,
    #[serde_as(as = "Hex")]
    pub blockhash: [u8; 32],
    #[serde_as(as = "Vec<Hex>")]
    pub transactions: Vec<Vec<u8>>,
    pub id: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::types::tracer_ch_common::RevisionMap;

    #[test]
    fn test_build_ranges_empty() {
        let results = Vec::new();
        let exp = Vec::new();
        let res = RevisionMap::build_ranges(&results);
        assert_eq!(res, exp);
    }

    #[test]
    fn test_build_ranges_single_element() {
        let results = vec![(1u64, String::from("Rev1"))];
        let exp = vec![(1u64, 2u64, String::from("Rev1"))];
        let res = RevisionMap::build_ranges(&results);
        assert_eq!(res, exp);
    }

    #[test]
    fn test_build_ranges_multiple_elements_different_revision() {
        let results = vec![
            (222_222_222u64, String::from("Rev1")),
            (333_333_333u64, String::from("Rev2")),
            (444_444_444u64, String::from("Rev3")),
        ];

        let exp = vec![
            (222_222_222u64, 333_333_333u64, String::from("Rev1")),
            (333_333_334u64, 444_444_444u64, String::from("Rev2")),
            (444_444_445u64, 444_444_445u64, String::from("Rev3")),
        ];
        let res = RevisionMap::build_ranges(&results);

        assert_eq!(res, exp);
    }

    #[test]
    fn test_rangemap() {
        let ranges = vec![
            (123_456_780, 123_456_788, String::from("Rev1")),
            (123_456_789, 123_456_793, String::from("Rev2")),
            (123_456_794, 123_456_799, String::from("Rev3")),
        ];
        let map = RevisionMap::new(ranges);

        assert_eq!(map.get(123_456_779), None); // Below the bottom bound of the first range

        assert_eq!(map.get(123_456_780), Some(String::from("Rev1"))); // The bottom bound of the first range
        assert_eq!(map.get(123_456_785), Some(String::from("Rev1"))); // Within the first range
        assert_eq!(map.get(123_456_788), Some(String::from("Rev1"))); // The top bound of the first range

        assert_eq!(map.get(123_456_793), Some(String::from("Rev2"))); // The bottom bound of the second range
        assert_eq!(map.get(123_456_790), Some(String::from("Rev2"))); // Within the second range
        assert_eq!(map.get(123_456_793), Some(String::from("Rev2"))); // The top bound of the second range

        assert_eq!(map.get(123_456_799), Some(String::from("Rev3"))); // The bottom bound of the third range
        assert_eq!(map.get(123_456_795), Some(String::from("Rev3"))); // Within the third range
        assert_eq!(map.get(123_456_799), Some(String::from("Rev3"))); // The top bound of the third range

        assert_eq!(map.get(123_456_800), None); // Beyond the top end of the last range
    }

    #[test]
    fn test_deserialize() {
        let txt = r#"
        {
            "step_limit": 500000,
            "accounts": [],
            "chains": [
                {
                    "id": 111,
                    "name": "neon",
                    "token": "HPsV9Deocecw3GeZv1FkAPNCBRfuVyfw9MMwjwRe1xaU"
                },
                {
                    "id": 112,
                    "name": "sol",
                    "token": "So11111111111111111111111111111111111111112"
                },
                {
                    "id": 113,
                    "name": "usdt",
                    "token": "2duuuuhNJHUYqcnZ7LKfeufeeTBgSJdftf2zM3cZV6ym"
                },
                {
                    "id": 114,
                    "name": "eth",
                    "token": "EwJYd3UAFAgzodVeHprB2gMQ68r4ZEbbvpoVzCZ1dGq5"
                }
            ],
            "tx": {
                "from": "0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99",
                "to": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
                "value": "0x0",
                "data": "3ff21f8e",
                "chain_id": 111
            },
            "solana_overrides": {
                "EwJYd3UAFAgzodVeHprB2gMQ68r4ZEbbvpoVzCZ1dGq5": null,
                "2duuuuhNJHUYqcnZ7LKfeufeeTBgSJdftf2zM3cZV6ym": {
                    "lamports": 1000000000000,
                    "owner": "So11111111111111111111111111111111111111112",
                    "executable": false,
                    "rent_epoch": 0,
                    "data": "0102030405"
                }
            },
            "provide_account_info": null
        }
        "#;

        let request: super::EmulateRequest = serde_json::from_str(txt).unwrap();
        println!("{request:?}");
    }
}
