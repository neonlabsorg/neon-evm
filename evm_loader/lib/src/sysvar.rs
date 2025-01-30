use crate::{rpc::Rpc, NeonError, NeonResult};
use solana_sdk::sysvar::{Sysvar, SysvarId};

pub async fn get_sysvar<T>(rpc: &impl Rpc) -> NeonResult<T>
where
    T: Sysvar + SysvarId,
{
    let account = rpc
        .get_account(&T::id())
        .await?
        .ok_or(NeonError::AccountNotFound(T::id()))?;

    let sysvar = bincode::deserialize::<T>(&account.data)?;
    Ok(sysvar)
}
