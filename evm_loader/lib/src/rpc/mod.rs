mod db_call_client;
mod emulator_client;
mod validator_client;
use crate::commands::get_config::GetConfigResponse;

pub use db_call_client::CallDbClient;
use solana_sdk::sysvar::{Sysvar, SysvarId};
pub use validator_client::CloneRpcClient;

use crate::commands::get_config::{BuildConfigSimulator, ConfigSimulator};
use crate::{NeonError, NeonResult};
use async_trait::async_trait;

use crate::types::programs_cache::get_program_programdata_address;
use crate::types::programs_cache::get_programdata_slot_from_account;

use enum_dispatch::enum_dispatch;
use evm_loader::solana_program::bpf_loader_upgradeable::UpgradeableLoaderState;
pub use solana_account_decoder::UiDataSliceConfig as SliceConfig;
use solana_cli::cli::CliError;
use solana_client::client_error::{ClientErrorKind, Result as ClientResult};
use solana_sdk::{
    account::Account, message::Message, native_token::lamports_to_sol, pubkey::Pubkey,
};

#[async_trait(?Send)]
#[enum_dispatch]
pub trait Rpc {
    async fn get_account_slice(
        &self,
        key: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>>;

    async fn get_last_deployed_slot(&self, program_id: &Pubkey) -> ClientResult<Option<u64>> {
        let mut slice_len = std::mem::size_of::<UpgradeableLoaderState>();
        if slice_len < UpgradeableLoaderState::size_of_programdata_metadata() {
            slice_len = UpgradeableLoaderState::size_of_programdata_metadata();
        }

        let slice = SliceConfig {
            offset: 0,
            length: slice_len,
        };

        let result = self.get_account_slice(program_id, Some(slice)).await;
        // bpfv2 and request account from link

        if let Ok(Some(acc)) = result {
            let slot = if acc.executable {
                get_programdata_slot_from_account(&acc)
                    .expect("error")
                    .expect("No slot info")
            } else {
                let pd_addr = get_program_programdata_address(&acc)?.expect("no program info");
                let rz = self
                    .get_account_slice(&pd_addr, Some(slice))
                    .await?
                    .expect("No account ");
                get_programdata_slot_from_account(&rz)?.expect("No slice ")
            };
            return Ok(Some(slot));
        }
        Err(ClientErrorKind::Custom("Not account on slot ".to_string()).into())
    }

    async fn get_account(&self, key: &Pubkey) -> ClientResult<Option<Account>> {
        self.get_account_slice(key, None).await
    }

    async fn get_multiple_accounts(&self, pubkeys: &[Pubkey])
        -> ClientResult<Vec<Option<Account>>>;

    async fn get_sysvar<T>(&self) -> NeonResult<T>
    where
        T: Sysvar + SysvarId,
    {
        let account = self
            .get_account(&T::id())
            .await?
            .ok_or(NeonError::AccountNotFound(T::id()))?;

        let sysvar = bincode::deserialize::<T>(&account.data)?;
        Ok(sysvar)
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>>;
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
