use crate::account_data::AccountData;
use crate::config::RocksDbConfig;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::Serialize;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

#[allow(dead_code)]
use crate::types::tracer_db_rpc_api::TracerDbApiClient;
use solana_account_decoder::UiDataSliceConfig;
use solana_sdk::signature::Signature;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
use std::env;

use std::sync::Arc;
use tracing::{debug, info};

#[derive(Clone, Serialize)]
pub struct AccountParams {
    pub pubkey: Pubkey,
    pub slot: u64,
    pub tx_index_in_block: Option<u64>,
}

use crate::types::tracer_ch_common::{EthSyncStatus, RevisionMap};
use crate::types::tracer_db_rpc_api::{Bs58Vec, SolanaReadableAccount};
use crate::types::{DbResult, TracerDbTrait};

#[derive(Clone, Debug)]
pub struct RocksDb {
    #[allow(dead_code)]
    url: String,
    client: Arc<WsClient>,
}

impl RocksDb {
    #[must_use]
    pub async fn new(config: &RocksDbConfig) -> Self {
        let host = &config.rocksdb_host;
        let port = &config.rocksdb_port;
        let url = format!("ws://{host}:{port}");
        match WsClientBuilder::default().build(&url).await {
            Ok(client) => {
                let arc_c = Arc::new(client);
                tracing::info!("Created rocksdb client at {url}");
                Self { url, client: arc_c }
            }
            Err(e) => panic!("Couldn't start rocksDb client at {url}: {e}"),
        }
    }
}

#[async_trait]
impl TracerDbTrait for RocksDb {
    async fn get_block_time(&self, slot: Slot) -> DbResult<UnixTimestamp> {
        self.client
            .get_block_time(slot)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Block time value is None"))
    }

    async fn get_earliest_rooted_slot(&self) -> DbResult<u64> {
        Ok(self.client.get_earliest_rooted_slot().await?)
    }

    async fn get_latest_block(&self) -> DbResult<u64> {
        Ok(self.client.get_earliest_rooted_slot().await?)
    }

    async fn get_account_at(
        &self,
        pubkey: &Pubkey,
        slot: u64,
        tx_index_in_block: Option<u64>,
        maybe_bin_slice: Option<UiDataSliceConfig>,
    ) -> DbResult<Option<Account>> {
        info!("get_account_at {pubkey:?}, slot: {slot:?}, tx_index: {tx_index_in_block:?}, bin_slice: {maybe_bin_slice:?}");
        self.client
            .get_account_at(
                Bs58Vec::from(pubkey.to_bytes().to_vec()),
                slot,
                tx_index_in_block,
                maybe_bin_slice,
            )
            .await?
            .map(Account::try_from)
            .transpose()
    }

    async fn get_transaction_index(&self, signature: Signature) -> DbResult<u64> {
        let tx_index = self
            .client
            .get_transaction_index(Bs58Vec::from(signature.as_ref().to_vec()))
            .await?;
        tx_index.ok_or_else(|| anyhow::anyhow!("get_transaction_index value is None"))
    }

    async fn get_neon_revisions(&self, _pubkey: &Pubkey) -> DbResult<RevisionMap> {
        let revision = env::var("NEON_REVISION").expect("NEON_REVISION should be set");

        info!("get_neon_revisions for {revision:?}");
        let ranges = vec![(1, 100_000, revision)];
        Ok(RevisionMap::new(ranges))
    }

    async fn get_neon_revision(&self, slot: Slot, pubkey: &Pubkey) -> DbResult<String> {
        info!("get_neon_revision for {slot:?}, pubkey: {pubkey:?}");
        let neon_revision = env!("NEON_REVISION");
        Ok(neon_revision.to_string())
    }

    async fn get_slot_by_blockhash(&self, blockhash: String) -> DbResult<u64> {
        self.client
            .get_slot_by_blockhash(blockhash.as_str())
            .await?
            .ok_or_else(|| anyhow!("get_slot_by_blockhash value is None"))
    }

    async fn get_sync_status(&self) -> DbResult<EthSyncStatus> {
        Ok(EthSyncStatus::new(None))
    }

    async fn get_accounts_in_transaction(
        &self,
        sol_sig: &[u8],
        slot: u64,
    ) -> DbResult<Vec<AccountData>> {
        // Convert the signature to Bs58Vec (assuming it's a 64-byte Solana Signature)
        let response: Vec<(Bs58Vec, SolanaReadableAccount)> = self
            .client
            .get_accounts_in_transaction(Bs58Vec::from(sol_sig.to_vec()), Some(slot))
            .await?;

        debug!("Accounts in response: {:?}", response);

        let account_data_vec: Vec<AccountData> = response
            .into_iter()
            .map(|(pubkey, acc)| -> Result<_, anyhow::Error> {
                let pubkey_array: [u8; 32] = pubkey.bytes[0..32]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid pubkey length"))?;

                let pk = Pubkey::new_from_array(pubkey_array);
                let acc: Account = acc.try_into()?; // assumes TryFrom<SolanaReadableAccount> for Account
                Ok(AccountData::new_from_account(pk, &acc))
            })
            .collect::<Result<_, _>>()?;
        Ok(account_data_vec)
    }
}
