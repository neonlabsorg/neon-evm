use serde_with::serde_as;
use solana_account_decoder::UiDataSliceConfig;
use solana_sdk::bs58;

use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::base64::Base64;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;

use serde_with::{DeserializeAs, SerializeAs};
use solana_sdk::account::Account;
use solana_sdk::clock::Epoch;
use solana_sdk::pubkey::Pubkey;

pub struct Base58Array<const N: usize>;

impl<const N: usize> SerializeAs<[u8; N]> for Base58Array<N> {
    fn serialize_as<S>(source: &[u8; N], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = bs58::encode(source).into_string();
        serializer.serialize_str(&encoded)
    }
}

impl<'de, const N: usize> DeserializeAs<'de, [u8; N]> for Base58Array<N> {
    fn deserialize_as<D>(deserializer: D) -> Result<[u8; N], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let decoded = bs58::decode(&s)
            .into_vec()
            .map_err(serde::de::Error::custom)?;
        let tmp_len = decoded.len();
        decoded.try_into().map_err(|_| {
            #[allow(clippy::uninlined_format_args)]
            serde::de::Error::custom(format!(
                "Expected base58-encoded {} bytes, got {}",
                N, tmp_len
            ))
        })
    }
}
#[serde_as]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PubkeyBase58(#[serde_as(as = "Base58Array<32>")] pub [u8; 32]);

impl From<Pubkey> for PubkeyBase58 {
    fn from(pubkey: Pubkey) -> Self {
        Self(pubkey.to_bytes())
    }
}
impl From<PubkeyBase58> for Pubkey {
    fn from(pubkey_base58: PubkeyBase58) -> Self {
        Self::new_from_array(pubkey_base58.0)
    }
}
#[serde_as]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct BlockHashBase58(#[serde_as(as = "Base58Array<32>")] pub [u8; 32]);

impl From<Hash> for BlockHashBase58 {
    fn from(hash: Hash) -> Self {
        Self(hash.to_bytes())
    }
}
impl From<BlockHashBase58> for Hash {
    fn from(block_hash_base58: BlockHashBase58) -> Self {
        Self::new_from_array(block_hash_base58.0)
    }
}
// impl From<&String> for BlockHashBase58 {
//     fn from(hash: &String) -> Self {
//         let bytes = bs58::decode(hash).into_vec().expect("Invalid base58 hash");
//         assert_eq!(bytes.len(), 32, "Expected 32-byte hash");
//         let mut array = [0u8; 32];
//         array.copy_from_slice(&bytes);
//         Self(array)
//     }
// }

#[serde_as]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SignatureBase58(#[serde_as(as = "Base58Array<64>")] [u8; 64]);

impl From<Signature> for SignatureBase58 {
    fn from(sig: Signature) -> Self {
        Self(sig.as_ref().try_into().expect("Signature must be 64 bytes"))
    }
}

impl From<[u8; 64]> for SignatureBase58 {
    fn from(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

impl From<SignatureBase58> for Signature {
    fn from(signature_base58: SignatureBase58) -> Self {
        Self::from(signature_base58.0)
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SolanaReadableAccount {
    pub lamports: u64,
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
    pub owner: PubkeyBase58,
    pub executable: bool,
    pub rent_epoch: Epoch,
}
impl From<Account> for SolanaReadableAccount {
    fn from(account: Account) -> Self {
        let data = account.data.clone();
        Self {
            lamports: account.lamports,
            data,
            owner: PubkeyBase58::from(account.owner),
            executable: account.executable,
            rent_epoch: account.rent_epoch,
        }
    }
}

impl TryFrom<SolanaReadableAccount> for Account {
    type Error = anyhow::Error;
    fn try_from(account: SolanaReadableAccount) -> Result<Self, Self::Error> {
        Ok(Self {
            lamports: account.lamports,
            data: account.data,
            owner: Pubkey::from(account.owner),
            executable: account.executable,
            rent_epoch: account.rent_epoch,
        })
    }
}

// API
#[rpc(client, server)]
#[async_trait]
pub trait TracerDbApi {
    #[method(name = "get_account")]
    async fn get_account(
        &self,
        pubkey: PubkeyBase58,
        slot: u64,
        write_version: Option<u64>,
        bindata: Option<UiDataSliceConfig>,
    ) -> RpcResult<Option<SolanaReadableAccount>>;
    #[method(name = "get_block_time")]
    async fn get_block_time(&self, slot: u64) -> RpcResult<Option<i64>>;
    #[method(name = "get_first_received_slot")]
    async fn get_first_received_slot(&self) -> RpcResult<u64>;
    #[method(name = "get_last_received_slot")]
    async fn get_last_received_slot(&self) -> RpcResult<u64>;

    #[method(name = "get_earliest_rooted_slot")]
    async fn get_earliest_rooted_slot(&self) -> RpcResult<u64>;

    #[method(name = "get_last_rooted_slot")]
    async fn get_last_rooted_slot(&self) -> RpcResult<u64>;
    #[method(name = "get_slot_by_blockhash")]
    async fn get_slot_by_blockhash(&self, hash: BlockHashBase58) -> RpcResult<Option<u64>>;

    #[method(name = "get_transaction_index")]
    async fn get_transaction_index(&self, signature: SignatureBase58) -> RpcResult<Option<u64>>;

    #[method(name = "get_accounts")]
    async fn get_accounts(&self, start: u64, end: u64) -> RpcResult<Vec<PubkeyBase58>>;

    #[method(name = "get_accounts_in_transaction")]
    async fn get_accounts_in_transaction(
        &self,
        signature: SignatureBase58,
        slot: Option<u64>,
    ) -> RpcResult<Vec<(PubkeyBase58, SolanaReadableAccount)>>;

    #[method(name = "copy_account")]
    async fn copy_account(
        &self,
        pubkey: PubkeyBase58,
        slot: u64,
        write_version: Option<u64>,
        new_slot: u64,
    ) -> RpcResult<()>;
    #[method(name = "get_account_data_history")]
    async fn get_account_data_history(
        &self,
        pubkey: PubkeyBase58,
        slot_from: Option<u64>,
        slot_to: Option<u64>,
    ) -> RpcResult<Vec<(u64, u64)>>;
}
