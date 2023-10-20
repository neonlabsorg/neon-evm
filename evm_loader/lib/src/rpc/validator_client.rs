use super::Rpc;
use async_trait::async_trait;
use solana_client::{
    client_error::Result as ClientResult,
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcSendTransactionConfig, RpcTransactionConfig},
    rpc_response::RpcResult,
};
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    commitment_config::CommitmentConfig,
    hash::Hash,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use solana_transaction_status::{
    EncodedConfirmedBlock, EncodedConfirmedTransactionWithStatusMeta, TransactionStatus,
};
use std::any::Any;

#[async_trait(?Send)]
impl Rpc for RpcClient {
    fn commitment(&self) -> CommitmentConfig {
        self.commitment()
    }

    async fn confirm_transaction_with_spinner(
        &self,
        signature: &Signature,
        recent_blockhash: &Hash,
        commitment_config: CommitmentConfig,
    ) -> ClientResult<()> {
        self.confirm_transaction_with_spinner(signature, recent_blockhash, commitment_config)
            .await
    }

    async fn get_account(&self, key: &Pubkey) -> ClientResult<Account> {
        self.get_account(key).await
    }

    async fn get_account_with_commitment(
        &self,
        key: &Pubkey,
        commitment: CommitmentConfig,
    ) -> RpcResult<Option<Account>> {
        self.get_account_with_commitment(key, commitment).await
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        self.get_multiple_accounts(pubkeys).await
    }

    async fn get_account_data(&self, key: &Pubkey) -> ClientResult<Vec<u8>> {
        Ok(self.get_account(key).await?.data)
    }

    async fn get_block(&self, slot: Slot) -> ClientResult<EncodedConfirmedBlock> {
        self.get_block(slot).await
    }

    async fn get_block_time(&self, slot: Slot) -> ClientResult<UnixTimestamp> {
        self.get_block_time(slot).await
    }

    async fn get_latest_blockhash(&self) -> ClientResult<Hash> {
        self.get_latest_blockhash().await
    }

    async fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> ClientResult<u64> {
        self.get_minimum_balance_for_rent_exemption(data_len).await
    }

    async fn get_slot(&self) -> ClientResult<Slot> {
        self.get_slot().await
    }

    async fn get_signature_statuses(
        &self,
        signatures: &[Signature],
    ) -> RpcResult<Vec<Option<TransactionStatus>>> {
        self.get_signature_statuses(signatures).await
    }

    async fn get_transaction_with_config(
        &self,
        signature: &Signature,
        config: RpcTransactionConfig,
    ) -> ClientResult<EncodedConfirmedTransactionWithStatusMeta> {
        self.get_transaction_with_config(signature, config).await
    }

    async fn send_transaction(&self, transaction: &Transaction) -> ClientResult<Signature> {
        self.send_transaction(transaction).await
    }

    async fn send_and_confirm_transaction_with_spinner(
        &self,
        transaction: &Transaction,
    ) -> ClientResult<Signature> {
        self.send_and_confirm_transaction_with_spinner(transaction)
            .await
    }

    async fn send_and_confirm_transaction_with_spinner_and_commitment(
        &self,
        transaction: &Transaction,
        commitment: CommitmentConfig,
    ) -> ClientResult<Signature> {
        self.send_and_confirm_transaction_with_spinner_and_commitment(transaction, commitment)
            .await
    }

    async fn send_and_confirm_transaction_with_spinner_and_config(
        &self,
        transaction: &Transaction,
        commitment: CommitmentConfig,
        config: RpcSendTransactionConfig,
    ) -> ClientResult<Signature> {
        self.send_and_confirm_transaction_with_spinner_and_config(transaction, commitment, config)
            .await
    }

    async fn get_latest_blockhash_with_commitment(
        &self,
        commitment: CommitmentConfig,
    ) -> ClientResult<(Hash, u64)> {
        self.get_latest_blockhash_with_commitment(commitment).await
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
