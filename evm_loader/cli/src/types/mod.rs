mod indexer_db;
pub mod request_models;
#[allow(clippy::all)]
pub mod trace;
mod tracer_ch_db;

pub use indexer_db::IndexerDb;
use lazy_static::lazy_static;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tokio::{runtime::Runtime, task::block_in_place};
pub use tracer_ch_db::{ChError, ChResult, ClickHouseDb as TracerDb};

use {
    crate::types::trace::{TraceCallConfig, TraceConfig},
    ethnum::U256,
    evm_loader::types::Address,
    hex::FromHex,
    postgres::NoTls,
    serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer},
    std::{fmt, ops::Deref},
    thiserror::Error,
    // tokio::task::block_in_place,
    tokio_postgres::{connect, Client},
};

/// Wrapper structure around vector of bytes.
#[derive(Debug, PartialEq, Eq, Default, Hash, Clone)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    /// Simple constructor.
    pub fn new(bytes: Vec<u8>) -> Bytes {
        Bytes(bytes)
    }
}

impl Deref for Bytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Bytes {
        Bytes(bytes)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(value: Bytes) -> Self {
        value.0
    }
}

impl Serialize for Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = "0x".to_owned();
        value.push_str(hex::encode(&self.0).as_str());
        serializer.serialize_str(value.as_ref())
    }
}

impl<'a> Deserialize<'a> for Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'a>,
    {
        deserializer.deserialize_any(BytesVisitor)
    }
}

struct BytesVisitor;

impl<'a> Visitor<'a> for BytesVisitor {
    type Value = Bytes;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a 0x-prefixed, hex-encoded vector of bytes")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.len() >= 2 && value.starts_with("0x") && value.len() & 1 == 0 {
            Ok(Bytes::new(FromHex::from_hex(&value[2..]).map_err(|e| {
                serde::de::Error::custom(format!("Invalid hex: {}", e))
            })?))
        } else {
            Err(serde::de::Error::custom(
                "Invalid bytes format. Expected a 0x-prefixed hex string with even length",
            ))
        }
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(value.as_ref())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct ChDbConfig {
    pub clickhouse_url: Vec<String>,
    pub clickhouse_user: Option<String>,
    pub clickhouse_password: Option<String>,
    pub indexer_host: String,
    pub indexer_port: String,
    pub indexer_database: String,
    pub indexer_user: String,
    pub indexer_password: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TxParams {
    pub nonce: Option<u64>,
    pub from: Address,
    pub to: Option<Address>,
    pub data: Option<Vec<u8>>,
    pub value: Option<U256>,
    pub gas_limit: Option<U256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionParams {
    pub data: Option<Bytes>,
    pub trace_config: Option<TraceCallConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionHashParams {
    pub trace_config: Option<TraceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceNextBlockParams {
    pub trace_config: Option<TraceConfig>,
}

pub fn do_connect(
    host: &String,
    port: &String,
    db: &String,
    user: &String,
    pass: &String,
) -> Client {
    let authority = format!("host={host} port={port} dbname={db} user={user} password={pass}");

    let mut attempt = 0;
    let mut result = None;

    while attempt < 3 {
        result = block(|| async { connect(&authority, NoTls).await }).ok();
        if result.is_some() {
            break;
        }
        attempt += 1;
    }

    let (client, connection) = result.expect("error to set DB connection");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {e}");
        }
    });
    client
}

lazy_static! {
    pub static ref RT: Runtime = tokio::runtime::Runtime::new().unwrap();
}

pub fn block<F, Fu, R>(f: F) -> R
where
    F: FnOnce() -> Fu,
    Fu: std::future::Future<Output = R>,
{
    block_in_place(|| RT.block_on(f()))
}

#[derive(Error, Debug)]
pub enum PgError {
    #[error("postgres: {}", .0)]
    Db(#[from] tokio_postgres::Error),
    #[error("Custom: {0}")]
    Custom(String),
}

pub type PgResult<T> = std::result::Result<T, PgError>;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct PubkeyBase58(
    #[serde(serialize_with = "crate::types::serde_pubkey_bs58")]
    #[serde(deserialize_with = "crate::types::deserialize_pubkey_from_str")]
    pub Pubkey,
);

impl AsRef<Pubkey> for PubkeyBase58 {
    fn as_ref(&self) -> &Pubkey {
        &self.0
    }
}

impl From<Pubkey> for PubkeyBase58 {
    fn from(value: Pubkey) -> Self {
        Self(value)
    }
}

impl From<&Pubkey> for PubkeyBase58 {
    fn from(value: &Pubkey) -> Self {
        Self(*value)
    }
}

impl From<PubkeyBase58> for Pubkey {
    fn from(value: PubkeyBase58) -> Self {
        value.0
    }
}

pub fn serde_pubkey_bs58<S>(value: &Pubkey, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let bs58 = bs58::encode(value).into_string();
    s.serialize_str(&bs58)
}

#[allow(unused)]
pub fn deserialize_pubkey_from_str<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringVisitor;
    impl<'de> serde::de::Visitor<'de> for StringVisitor {
        type Value = Pubkey;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string containing json data")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Pubkey::from_str(v).map_err(E::custom)
        }
    }
    deserializer.deserialize_any(StringVisitor)
}
