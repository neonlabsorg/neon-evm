pub mod deactivated_features;
pub mod programs_cache;
pub mod tracer_ch_common;
pub(crate) mod tracer_ch_db;
pub mod tracer_rocks_db;

use crate::account_data::AccountData;
use crate::commands::get_config::ChainInfo;
use crate::config::DbConfig;
use crate::tracing::TraceCallConfig;
use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};
pub use crate::types::tracer_ch_db::ClickHouseDb;
pub use crate::types::tracer_rocks_db::RocksDb;
use crate::NeonResult;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use ethnum::U256;
use evm_loader::platform::Platform;
use evm_loader::solana_program::clock::{Slot, UnixTimestamp};
pub use evm_loader::types::Address;
use evm_loader::types::Transaction;
use evm_loader::types::{ExecutionMap, TransactionType};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use serde_with::{hex::Hex, serde_as, DefaultOnNull, DisplayFromStr, OneOrMany};
use solana_sdk::account::{AccountSharedData, ReadableAccount};

use crate::rpc::SliceConfig;
use solana_sdk::signature::Signature;
use solana_sdk::{account::Account, pubkey::Pubkey};
use std::collections::HashMap;
use DbConfig::{ChDbConfig, RocksDbConfig};

pub type DbResult<T> = Result<T, anyhow::Error>;

#[enum_dispatch]
#[allow(clippy::large_enum_variant)]
pub enum TracerDb {
    ClickHouseDb,
    RocksDb,
}

impl TracerDb {
    pub async fn from_config(db_config: &DbConfig) -> Self {
        match db_config {
            RocksDbConfig(rocks_db_config) => RocksDb::new(rocks_db_config).await.into(),
            ChDbConfig(ch_db_config) => ClickHouseDb::new(ch_db_config).into(),
        }
    }

    pub async fn maybe_from_config(maybe_db_config: Option<&DbConfig>) -> Option<Self> {
        if let Some(db_config) = maybe_db_config {
            Some(Self::from_config(db_config).await)
        } else {
            None
        }
    }
}

impl Clone for TracerDb {
    fn clone(&self) -> Self {
        match self {
            Self::RocksDb(r) => r.clone().into(),
            Self::ClickHouseDb(c) => c.clone().into(),
        }
    }
}

#[async_trait]
#[enum_dispatch(TracerDb)]
pub trait TracerDbTrait {
    async fn get_block_time(&self, slot: Slot) -> DbResult<UnixTimestamp>;

    async fn get_earliest_rooted_slot(&self) -> DbResult<u64>;

    async fn get_latest_block(&self) -> DbResult<u64>;

    async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
        data_slice: Option<SliceConfig>,
    ) -> DbResult<Option<Account>>;

    async fn get_transaction_index(&self, signature: Signature) -> DbResult<u64>;

    async fn get_neon_revisions(&self, _pubkey: &Pubkey) -> DbResult<RevisionMap>;

    async fn get_neon_revision(&self, _slot: Slot, _pubkey: &Pubkey) -> DbResult<String>;

    async fn get_slot_by_blockhash(&self, blockhash: String) -> DbResult<u64>;

    async fn get_sync_status(&self) -> DbResult<EthSyncStatus>;

    async fn get_accounts_in_transaction(
        &self,
        sol_sig: &[u8],
        slot: u64,
    ) -> DbResult<Vec<AccountData>>;
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AccessListItem {
    pub address: Address,
    #[serde(rename = "storageKeys")]
    #[serde_as(as = "Vec<Hex>")]
    pub storage_keys: Vec<[u8; 32]>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum FromAddress {
    Ethereum(Address),
    Solana(Pubkey),
}

impl std::fmt::Display for FromAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ethereum(address) => address.fmt(f),
            Self::Solana(pubkey) => pubkey.fmt(f),
        }
    }
}

impl std::str::FromStr for FromAddress {
    type Err = solana_sdk::pubkey::ParsePubkeyError;

    #[allow(clippy::option_if_let_else)] // map_or_else makes code unreadable
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(address) = s.parse::<Address>() {
            Ok(Self::Ethereum(address))
        } else if let Ok(pubkey) = s.parse::<Pubkey>() {
            Ok(Self::Solana(pubkey))
        } else {
            Err(solana_sdk::pubkey::ParsePubkeyError::Invalid)
        }
    }
}

impl FromAddress {
    #[must_use]
    pub fn address(&self) -> Address {
        match self {
            Self::Ethereum(address) => *address,
            Self::Solana(pubkey) => Address::from_solana_address(pubkey),
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Serialize, Deserialize)]
pub struct TxParams {
    pub nonce: Option<u64>,
    pub index: Option<u16>,
    #[serde_as(as = "DisplayFromStr")]
    pub from: FromAddress,
    pub payer: Option<Address>,
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
    pub async fn fetch_origin_nonce(&mut self, platform: &impl Platform) -> NeonResult<()> {
        if self.nonce.is_some() {
            return Ok(());
        }

        let origin = self.from.address();
        let chain_id = self.chain_id.unwrap_or_else(|| platform.default_chain());

        let Some(balance) = platform.get_balance(origin, chain_id).await? else {
            return Ok(());
        };

        self.nonce = Some(balance.nonce());

        Ok(())
    }
}

impl Transaction for TxParams {
    fn transaction_type(&self) -> TransactionType {
        TransactionType::Legacy
    }

    fn hash(&self) -> &[u8; 32] {
        &[0_u8; 32]
    }

    fn chain_id(&self) -> Option<u64> {
        self.chain_id
    }

    fn nonce(&self) -> u64 {
        self.nonce.unwrap_or_default()
    }

    fn gas_limit(&self) -> U256 {
        self.gas_limit.unwrap_or_default()
    }

    fn gas_price(&self) -> U256 {
        self.gas_price.or(self.max_fee_per_gas).unwrap_or_default()
    }

    fn target(&self) -> Option<&Address> {
        self.to.as_ref()
    }

    fn call_data(&self) -> &[u8] {
        self.data.as_deref().unwrap_or_default()
    }

    fn value(&self) -> U256 {
        self.value.unwrap_or_default()
    }

    fn recover_caller_address(&self) -> evm_loader::error::Result<Address> {
        Ok(self.from.address())
    }
}

impl TxParams {
    #[must_use]
    pub fn from_transaction(origin: Address, tx: &dyn Transaction) -> Self {
        let mut payer = Some(origin);
        let mut index = None;

        let mut max_fee_per_gas = None;
        let mut max_priority_fee_per_gas = None;

        if let Some(tx) = tx.as_priority_fee() {
            max_fee_per_gas = Some(tx.max_fee_per_gas());
            max_priority_fee_per_gas = Some(tx.max_priority_fee_per_gas());
        }
        if let Some(tx) = tx.as_scheduled() {
            payer = Some(*tx.payer());
            index = Some(tx.index());
        }

        Self {
            from: FromAddress::Ethereum(origin),
            payer,
            to: tx.target().copied(),
            nonce: Some(tx.nonce()),
            index,
            data: Some(tx.call_data().to_vec()),
            value: Some(tx.value()),
            gas_limit: Some(tx.gas_limit()),
            gas_price: Some(tx.gas_price()),
            max_fee_per_gas,
            max_priority_fee_per_gas,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializedAccount {
    pub lamports: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub owner: Pubkey,
    pub executable: bool,
    pub rent_epoch: u64,
    #[serde_as(as = "Hex")]
    pub data: Vec<u8>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializedProgram {
    #[serde_as(as = "DisplayFromStr")]
    pub loader: Pubkey,
    #[serde_as(as = "Hex")]
    pub elf: Vec<u8>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializedAccountMeta {
    #[serde_as(as = "DisplayFromStr")]
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializedInstruction {
    #[serde_as(as = "DisplayFromStr")]
    pub program_id: Pubkey,
    #[serde_as(as = "DefaultOnNull")]
    pub accounts: Vec<SerializedAccountMeta>,
    #[serde_as(as = "DefaultOnNull<Hex>")]
    pub data: Vec<u8>,
}

impl From<SerializedAccountMeta> for solana_sdk::instruction::AccountMeta {
    fn from(s: SerializedAccountMeta) -> Self {
        Self {
            pubkey: s.pubkey,
            is_signer: s.is_signer,
            is_writable: s.is_writable,
        }
    }
}

impl From<SerializedInstruction> for solana_sdk::instruction::Instruction {
    fn from(s: SerializedInstruction) -> Self {
        let accounts = s
            .accounts
            .into_iter()
            .map(solana_sdk::instruction::AccountMeta::from)
            .collect();

        Self {
            program_id: s.program_id,
            accounts,
            data: s.data,
        }
    }
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

impl From<Account> for SerializedAccount {
    fn from(account: Account) -> Self {
        Self {
            lamports: account.lamports,
            owner: account.owner,
            executable: account.executable,
            rent_epoch: account.rent_epoch,
            data: account.data,
        }
    }
}

impl From<AccountData> for SerializedAccount {
    fn from(account: AccountData) -> Self {
        Self {
            lamports: account.lamports,
            owner: account.owner,
            executable: account.executable,
            rent_epoch: account.rent_epoch,
            data: account.data().to_vec(),
        }
    }
}

impl From<AccountSharedData> for SerializedAccount {
    fn from(account: AccountSharedData) -> Self {
        Self {
            lamports: account.lamports(),
            owner: *account.owner(),
            executable: account.executable(),
            rent_epoch: account.rent_epoch(),
            data: account.data().to_vec(),
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountInfoLevel {
    Changed,
    All,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateRequest {
    pub tx: TxParams,
    pub step_limit: Option<u64>,
    pub account_limit: Option<usize>,
    pub chains: Option<Vec<ChainInfo>>,
    pub trace_config: Option<TraceCallConfig>,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub accounts: Vec<Pubkey>,
    #[serde_as(as = "Option<HashMap<DisplayFromStr,_>>")]
    pub solana_overrides: Option<HashMap<Pubkey, Option<SerializedAccount>>>,
    pub provide_account_info: Option<AccountInfoLevel>,
    pub execution_map: Option<ExecutionMap>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateFromHolderApiRequest {
    #[serde_as(as = "DisplayFromStr")]
    pub holder_pubkey: Pubkey,
    pub step_limit: Option<u64>,
    pub account_limit: Option<usize>,
    pub chains: Option<Vec<ChainInfo>>,
    pub slot: Option<u64>,
    pub tx_index_in_block: Option<u64>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateApiRequest {
    #[serde(flatten)]
    pub body: EmulateRequest,
    pub slot: Option<u64>,
    pub tx_index_in_block: Option<u64>,
    pub id: Option<String>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateMultipleRequest {
    pub solana_tx: SimulateSolanaRequest,
    pub tx: Vec<TxParams>,
    pub step_limit: Option<u64>,
    pub account_limit: Option<usize>,
    pub chains: Option<Vec<ChainInfo>>,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub accounts: Vec<Pubkey>,
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
pub struct GetBalanceWithPubkeyRequest {
    #[serde_as(as = "OneOrMany<DisplayFromStr>")]
    pub account: Vec<Pubkey>,
    pub slot: Option<u64>,
    pub id: Option<String>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct GetTransactionTreeRequest {
    pub origin: BalanceAddress,
    pub nonce: u64,
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
pub struct GetContainerRequest {
    #[serde_as(as = "DisplayFromStr")]
    pub pubkey: Pubkey,
    pub slot: Option<u64>,
    pub id: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct SimulateSolanaRequest {
    pub compute_units: Option<u64>,
    pub heap_size: Option<u32>,
    pub instructions: Vec<SerializedInstruction>,
    pub id: Option<String>,
    #[serde_as(as = "Option<HashMap<DisplayFromStr,_>>")]
    pub accounts_overrides: Option<HashMap<Pubkey, SerializedAccount>>,
    #[serde_as(as = "Option<HashMap<DisplayFromStr,_>>")]
    pub programs_overrides: Option<HashMap<Pubkey, SerializedProgram>>,
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
            "provide_account_info": null,
            "execution_map": null
        }
        "#;

        let request: super::EmulateRequest = serde_json::from_str(txt).unwrap();
        println!("{request:?}");
    }
}
