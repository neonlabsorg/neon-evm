use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use solana_account_decoder::UiDataSliceConfig;
use solana_sdk::signature::Signature;

use solana_sdk::account::Account;
use solana_sdk::clock::Epoch;
use solana_sdk::pubkey::Pubkey;

fn serialize_base58<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let encoded = bs58::encode(bytes).into_string();
    serializer.serialize_str(&encoded)
}

fn deserialize_base58<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    bs58::decode(s).into_vec().map_err(serde::de::Error::custom)
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SolanaReadableAccount {
    lamports: u64,
    #[serde(
        serialize_with = "serialize_base58",
        deserialize_with = "deserialize_base58"
    )]
    // a slice so we don't have to make a copy just to serialize this
    data: Vec<u8>,
    #[serde(
        serialize_with = "serialize_base58",
        deserialize_with = "deserialize_base58"
    )]
    owner: Vec<u8>,
    executable: bool,
    rent_epoch: Epoch,
}
impl From<Account> for SolanaReadableAccount {
    fn from(account: Account) -> Self {
        let data = account.data.clone();
        Self {
            lamports: account.lamports,
            data,
            owner: account.owner.to_bytes().to_vec(),
            executable: account.executable,
            rent_epoch: account.rent_epoch,
        }
    }
}

impl TryFrom<SolanaReadableAccount> for Account {
    type Error = anyhow::Error;
    fn try_from(account: SolanaReadableAccount) -> Result<Self, Self::Error> {
        let owner_array: [u8; 32] = account
            .owner
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Owner field must be 32 bytes"))?;

        Ok(Self {
            lamports: account.lamports,
            data: account.data,
            owner: Pubkey::new_from_array(owner_array),
            executable: account.executable,
            rent_epoch: account.rent_epoch,
        })
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Bs58Vec {
    #[serde(
        serialize_with = "serialize_base58",
        deserialize_with = "deserialize_base58"
    )]
    pub bytes: Vec<u8>,
}
impl Bs58Vec {
    const fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}
impl From<Bs58Vec> for Vec<u8> {
    fn from(bs58_vec: Bs58Vec) -> Self {
        bs58_vec.bytes
    }
}

impl From<Vec<u8>> for Bs58Vec {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl From<Bs58Vec> for Pubkey {
    fn from(bs58_vec: Bs58Vec) -> Self {
        Self::new_from_array(bs58_vec.bytes.try_into().expect("Expected 32-byte pubkey"))
    }
}
impl From<Bs58Vec> for Signature {
    fn from(bs58_vec: Bs58Vec) -> Self {
        // Convert Vec<u8> into [u8; 64]
        let bytes: [u8; 64] = bs58_vec
            .bytes
            .try_into()
            .expect("Expected 64-byte signature");
        Self::from(bytes)
    }
}

// API
#[rpc(client, server)]
#[async_trait]
pub trait TracerDbApi {
    #[method(name = "get_account_at")]
    async fn get_account_at(
        &self,
        pubkey: Bs58Vec,
        slot: u64,
        write_version: Option<u64>,
        bindata: Option<UiDataSliceConfig>,
    ) -> jsonrpsee::core::RpcResult<Option<SolanaReadableAccount>>;
    #[method(name = "get_block_time")]
    async fn get_block_time(&self, slot: u64) -> RpcResult<Option<i64>>;
    #[method(name = "get_earliest_rooted_slot")]
    async fn get_earliest_rooted_slot(&self) -> RpcResult<u64>;
    #[method(name = "get_last_rooted_slot")]
    async fn get_last_rooted_slot(&self) -> RpcResult<u64>;
    #[method(name = "get_slot_by_blockhash")]
    async fn get_slot_by_blockhash(&self, hash: &str) -> RpcResult<Option<u64>>;
    #[method(name = "get_transaction_index")]
    async fn get_transaction_index(&self, signature: Bs58Vec) -> RpcResult<Option<u64>>;

    #[method(name = "get_accounts")]
    async fn get_accounts(&self, start: u64, end: u64) -> RpcResult<Vec<Bs58Vec>>;

    #[method(name = "get_accounts_in_transaction")]
    async fn get_accounts_in_transaction(
        &self,
        signature: Bs58Vec,
        slot: Option<u64>,
    ) -> RpcResult<Vec<(Bs58Vec, SolanaReadableAccount)>>;
}
