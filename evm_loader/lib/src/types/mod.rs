pub mod request_models;
pub mod tracer_ch_common;
mod tracer_ch_db;

pub use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;
pub use tracer_ch_db::ClickHouseDb as TracerDb;

use crate::commands::get_neon_elf::CachedElfParams;
use crate::RequestContext;
use evm_loader::evm::tracing::TraceCallConfig;
use evm_loader::types::hexbytes::HexBytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct ChDbConfig {
    pub clickhouse_url: Vec<String>,
    pub clickhouse_user: Option<String>,
    pub clickhouse_password: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AccessListItem {
    pub address: Address,
    pub storage_keys: Vec<HexBytes>,
}

pub struct EmulationParams {
    pub token_mint: Pubkey,
    pub chain_id: u64,
    pub max_steps_to_execute: u64,
    pub cached_accounts: Vec<Address>,
    pub solana_accounts: Vec<Pubkey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionParams {
    pub data: Option<HexBytes>,
    pub trace_config: Option<TraceCallConfig>,
}

pub async fn read_elf_params_if_none(
    context: &RequestContext<'_>,
    mut token_mint: Option<Pubkey>,
    mut chain_id: Option<u64>,
) -> (Pubkey, u64) {
    // Read ELF params only if token_mint or chain_id is not set.
    if token_mint.is_none() || chain_id.is_none() {
        let cached_elf_params = CachedElfParams::new(context).await;
        token_mint = token_mint.or_else(|| {
            Some(
                Pubkey::from_str(
                    cached_elf_params
                        .get("NEON_TOKEN_MINT")
                        .expect("NEON_TOKEN_MINT load error"),
                )
                .expect("NEON_TOKEN_MINT Pubkey ctor error "),
            )
        });
        chain_id = chain_id.or_else(|| {
            Some(
                u64::from_str(
                    cached_elf_params
                        .get("NEON_CHAIN_ID")
                        .expect("NEON_CHAIN_ID load error"),
                )
                .expect("NEON_CHAIN_ID u64 ctor error"),
            )
        });
    }

    (
        token_mint.expect("token_mint get error"),
        chain_id.expect("chain_id get error"),
    )
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PubkeyBase58(pub Pubkey);

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

impl Serialize for PubkeyBase58 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bs58 = bs58::encode(&self.0).into_string();
        serializer.serialize_str(&bs58)
    }
}

impl<'de> Deserialize<'de> for PubkeyBase58 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
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

        deserializer.deserialize_any(StringVisitor).map(Self)
    }
}
