use async_trait::async_trait;
use solana_account_decoder::UiDataSliceConfig as SliceConfig;
use solana_client::client_error::Result as ClientResult;
use solana_sdk::{account::Account, pubkey::Pubkey};

use crate::{emulator_platform::EmulatorPlatform, rpc::Rpc};

#[async_trait(?Send)]
impl<R: Rpc> Rpc for EmulatorPlatform<R> {
    async fn get_account_slice(
        &self,
        key: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>> {
        let Some(mut account) = self.get_account(key).await? else {
            return Ok(None);
        };

        if let Some(slice) = slice {
            let range = slice.offset..(slice.offset + slice.length);
            account.data.copy_within(range, 0);
            account.data.truncate(slice.length);
        }

        Ok(Some(account))
    }

    async fn get_account(&self, key: &Pubkey) -> ClientResult<Option<Account>> {
        let stack = self.current_stack_frame();
        if let Some(account) = stack.get(key) {
            Ok(Some(account.into()))
        } else {
            self.rpc.get_account(key).await
        }
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        if pubkeys.is_empty() {
            return Ok(Vec::new());
        }

        let mut accounts: Vec<Option<Account>> = vec![None; pubkeys.len()];

        let mut exists = vec![true; pubkeys.len()];
        let mut missing_keys = Vec::with_capacity(pubkeys.len());

        let stack = self.current_stack_frame();
        for (i, pubkey) in pubkeys.iter().enumerate() {
            if let Some(account_data) = stack.get(pubkey) {
                accounts[i] = Some(account_data.into());
                continue;
            }

            exists[i] = false;
            missing_keys.push(*pubkey);
        }

        let mut response = self.rpc.get_multiple_accounts(&missing_keys).await?;

        let mut j = 0_usize;
        for i in 0..pubkeys.len() {
            if exists[i] {
                continue;
            }

            assert_eq!(pubkeys[i], missing_keys[j]);
            accounts[i] = response[j].take();

            j += 1;
        }

        Ok(accounts)
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        self.rpc.get_deactivated_solana_features().await
    }
}
