mod cached_client;
mod db_call_client;
mod platform_client;
mod validator_client;
use crate::commands::get_config::GetConfigResponse;

pub use cached_client::CachedRpc;
pub use db_call_client::CallDbClient;
use tracing::trace;
pub use validator_client::CloneRpcClient;

use crate::commands::get_config::{BuildConfigSimulator, ConfigSimulator};
use crate::{NeonError, NeonResult};
use async_trait::async_trait;

use bincode::deserialize;
use enum_dispatch::enum_dispatch;
pub use solana_account_decoder::UiDataSliceConfig as SliceConfig;
use solana_cli::cli::CliError;
use solana_client::client_error::{ClientErrorKind, Result as ClientResult};
use solana_loader_v3_interface::state::UpgradeableLoaderState;
use solana_sdk::{
    account::Account, message::Message, native_token::lamports_to_sol, pubkey::Pubkey,
};
use solana_sdk_ids::{bpf_loader, bpf_loader_upgradeable};
use std::cmp::max;

#[async_trait(?Send)]
#[enum_dispatch]
pub trait Rpc {
    async fn get_account_slice(
        &self,
        key: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>>;

    async fn get_last_deployed_slot(&self, program_id: &Pubkey) -> ClientResult<Option<u64>> {
        let slice_len = max(
            std::mem::size_of::<UpgradeableLoaderState>(),
            UpgradeableLoaderState::size_of_programdata_metadata(),
        );
        let slice = SliceConfig {
            offset: 0,
            length: slice_len,
        };
        let result = self.get_account_slice(program_id, Some(slice)).await;

        if let Ok(Some(acc)) = result {
            // check if not upgradeable

            if bpf_loader::check_id(&acc.owner) {
                trace!("Account {program_id}  is  program not upgradeable");
                return Ok(Some(0));
            } else if bpf_loader_upgradeable::check_id(&acc.owner) {
                return match deserialize::<UpgradeableLoaderState>(&acc.data) {
                    Ok(UpgradeableLoaderState::Program {
                        programdata_address,
                        ..
                    }) => self.get_last_deployed_slot(&programdata_address).await,
                    Ok(UpgradeableLoaderState::ProgramData { slot, .. }) => {
                        trace!("Account {program_id}  is  programdata with slot {slot} ");
                        Ok(Some(slot))
                    }
                    Ok(_) => Err(ClientErrorKind::Custom(
                        "Not program nither programdata  ".to_string(),
                    )
                    .into()),
                    Err(_) => {
                        Err(ClientErrorKind::Custom("Data corruption error?  ".to_string()).into())
                    }
                };
            }
            trace!("Account {program_id} some troulbes and return None ");
            return Ok(None);
        }
        Err(ClientErrorKind::Custom("Not account on slot ".to_string()).into())
    }

    async fn get_account(&self, key: &Pubkey) -> ClientResult<Option<Account>> {
        self.get_account_slice(key, None).await
    }

    async fn get_multiple_accounts(&self, pubkeys: &[Pubkey])
        -> ClientResult<Vec<Option<Account>>>;

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>>;
}

#[async_trait(?Send)]
impl<R: Rpc> Rpc for &R {
    async fn get_account_slice(
        &self,
        key: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>> {
        Rpc::get_account_slice(*self, key, slice).await
    }

    async fn get_account(&self, key: &Pubkey) -> ClientResult<Option<Account>> {
        Rpc::get_account(*self, key).await
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        Rpc::get_multiple_accounts(*self, pubkeys).await
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        Rpc::get_deactivated_solana_features(*self).await
    }
}

#[enum_dispatch(BuildConfigSimulator, Rpc)]
pub enum RpcEnum {
    CloneRpcClient,
    CallDbClient,
}

macro_rules! e {
    ($mes:expr) => {
        ClientError::from(ClientErrorKind::Custom(format!("{}", $mes)))
    };
    ($mes:expr, $error:expr) => {
        ClientError::from(ClientErrorKind::Custom(format!("{}: {:?}", $mes, $error)))
    };
    ($mes:expr, $error:expr, $arg:expr) => {
        ClientError::from(ClientErrorKind::Custom(format!(
            "{}, {:?}: {:?}",
            $mes, $error, $arg
        )))
    };
}

pub(crate) use e;

pub(crate) async fn check_account_for_fee(
    rpc_client: &CloneRpcClient,
    account_pubkey: &Pubkey,
    message: &Message,
) -> NeonResult<()> {
    let fee = rpc_client.get_fee_for_message(message).await?;
    let balance = rpc_client.get_balance(account_pubkey).await?;
    if balance != 0 && balance >= fee {
        return Ok(());
    }

    Err(NeonError::CliError(CliError::InsufficientFundsForFee(
        lamports_to_sol(fee),
        *account_pubkey,
    )))
}
