use async_trait::async_trait;
use evm_loader::account_storage::AccountStorage;
use solana_client::{
    client_error::Result as ClientResult,
    rpc_response::{Response, RpcResponseContext, RpcResult},
};
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
};

use crate::account_storage::{fake_operator, EmulatorAccountStorage};

use super::Rpc;

#[async_trait(?Send)]
impl<'rpc, T: Rpc> Rpc for EmulatorAccountStorage<'rpc, T> {
    async fn get_account(&self, key: &Pubkey) -> RpcResult<Option<Account>> {
        let block_number = self.block_number().as_u64();
        let context = RpcResponseContext::new(block_number);

        if *key == self.operator() {
            return Ok(Response {
                context,
                value: Some(fake_operator()),
            });
        }

        if let Some(account_data) = self.accounts_get(key) {
            return Ok(Response {
                context,
                value: Some(Account::from(&*account_data)),
            });
        }

        let account = self._get_account_from_rpc(*key).await?.cloned();
        Ok(Response {
            context,
            value: account,
        })
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        let mut accounts = vec![None; pubkeys.len()];

        let mut exists = vec![true; pubkeys.len()];
        let mut missing_keys = Vec::with_capacity(pubkeys.len());

        for (i, pubkey) in pubkeys.iter().enumerate() {
            if pubkey == &self.operator() {
                accounts[i] = Some(fake_operator());
                continue;
            }

            if let Some(account_data) = self.accounts_get(pubkey) {
                accounts[i] = Some(Account::from(&*account_data));
                continue;
            }

            exists[i] = false;
            missing_keys.push(*pubkey);
        }

        let response = self._get_multiple_accounts_from_rpc(&missing_keys).await?;

        let mut j = 0_usize;
        for i in 0..pubkeys.len() {
            if exists[i] {
                continue;
            }

            assert_eq!(pubkeys[i], missing_keys[j]);
            accounts[i] = response[j].cloned();

            j += 1;
        }

        Ok(accounts)
    }

    async fn get_block_time(&self, _slot: Slot) -> ClientResult<UnixTimestamp> {
        Ok(self.block_timestamp().as_i64())
    }

    async fn get_slot(&self) -> ClientResult<Slot> {
        Ok(self.block_number().as_u64())
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        self._get_deactivated_solana_features().await
    }
}
