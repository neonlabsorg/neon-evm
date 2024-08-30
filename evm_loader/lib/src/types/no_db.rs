use super::tracer_ch_common::{EthSyncStatus, RevisionMap};
use crate::types::{DbResult, TracerDb};
use async_trait::async_trait;
use solana_sdk::signature::Signature;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};
#[derive(Clone, Debug, Default)]
pub struct NoDb {}

#[async_trait]
impl TracerDb for NoDb {
    async fn get_block_time(&self, slot: Slot) -> DbResult<UnixTimestamp> {
        Err(anyhow::anyhow!(
            "No DB configured for get_block_time({slot})"
        ))
    }

    async fn get_earliest_rooted_slot(&self) -> DbResult<u64> {
        Err(anyhow::anyhow!(
            "No DB configured for get_earliest_rooted_slot"
        ))
    }

    async fn get_latest_block(&self) -> DbResult<u64> {
        Err(anyhow::anyhow!("No DB configured for get_latest_block"))
    }

    async fn get_account_at(
        &self,
        _pubkey: &Pubkey,
        slot: u64,
        _tx_index_in_block: Option<u64>,
    ) -> DbResult<Option<Account>> {
        Err(anyhow::anyhow!(
            "No DB configured for get_account_at slot {slot}"
        ))
    }

    async fn get_transaction_index(&self, _signature: Signature) -> DbResult<u64> {
        Err(anyhow::anyhow!("No DB configured to get_transaction_index"))
    }

    async fn get_neon_revisions(&self, _pubkey: &Pubkey) -> DbResult<RevisionMap> {
        Err(anyhow::anyhow!(
            "No DB configured to get_neon_revisions for pubkey"
        ))
    }

    async fn get_neon_revision(&self, slot: Slot, _pubkey: &Pubkey) -> DbResult<String> {
        Err(anyhow::anyhow!(
            "No DB configured to get_neon_revisions for slot {slot}"
        ))
    }

    async fn get_slot_by_blockhash(&self, _blockhash: String) -> DbResult<u64> {
        Err(anyhow::anyhow!("No DB configured to get_slot_by_blockhash"))
    }

    async fn get_sync_status(&self) -> DbResult<EthSyncStatus> {
        Err(anyhow::anyhow!("No DB configured to get_sync_status"))
    }
}
