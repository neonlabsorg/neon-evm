use super::error::Error;
use log::debug;
use solana_program_runtime::sysvar_cache::SysvarCache;
use solana_sdk::{
    account::Account,
    account_utils::StateMut,
    address_lookup_table::{
        self,
        state::{AddressLookupTable, LookupTableMeta},
    },
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    pubkey::Pubkey,
    reserved_account_keys::ReservedAccountKeys,
    sysvar,
};

use crate::rpc::Rpc;

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum SyncState {
    No,
    Yes,
}

pub async fn sync_sysvar_accounts(
    rpc: &impl Rpc,
    sysvar_cache: &mut SysvarCache,
) -> Result<(), Error> {
    let keys: Vec<Pubkey> = ReservedAccountKeys::default().active.into_iter().collect();
    let mut accounts = rpc.get_multiple_accounts(&keys).await?;

    sysvar_cache.reset();

    for (account, key) in accounts.iter_mut().zip(keys) {
        let Some(account) = account else {
            continue;
        };

        sysvar_cache.fill_missing_entries(|pubkey, setter| match *pubkey {
            sysvar::clock::ID
            | sysvar::rent::ID
            | sysvar::epoch_rewards::ID
            | sysvar::epoch_schedule::ID
            | sysvar::slot_hashes::ID
            | sysvar::stake_history::ID
            | sysvar::last_restart_slot::ID => {
                if key == *pubkey {
                    setter(account.data.as_mut());
                }
            }
            #[allow(deprecated)]
            id if { sysvar::fees::check_id(&id) || sysvar::recent_blockhashes::check_id(&id) } => {
                if key == *pubkey {
                    setter(account.data.as_mut());
                }
            }
            _ => {}
        });
    }

    Ok(())
}

pub fn program_data_address(account: &Account) -> Result<Pubkey, Error> {
    assert!(account.executable);
    assert_eq!(account.owner, bpf_loader_upgradeable::id());

    let UpgradeableLoaderState::Program {
        programdata_address,
        ..
    } = account.state()?
    else {
        return Err(Error::ProgramAccountError);
    };

    Ok(programdata_address)
}

pub fn reset_program_data_slot(account: &mut Account) -> Result<(), Error> {
    assert_eq!(account.owner, bpf_loader_upgradeable::id());

    let UpgradeableLoaderState::ProgramData {
        slot,
        upgrade_authority_address,
    } = account.state()?
    else {
        return Err(Error::ProgramAccountError);
    };

    debug!(
        "slot_before_update: slot={slot} upgrade_authority_address={upgrade_authority_address:?}"
    );

    let new_state = UpgradeableLoaderState::ProgramData {
        slot: 0,
        upgrade_authority_address,
    };
    account.set_state(&new_state)?;

    debug!(
        "slot_after_update: slot={slot} upgrade_authority_address={upgrade_authority_address:?}"
    );

    Ok(())
}

pub fn reset_alt_slot(account: &mut Account) -> Result<(), Error> {
    assert_eq!(account.owner, address_lookup_table::program::id());

    let lookup_table = AddressLookupTable::deserialize(&account.data)?;
    let metadata = LookupTableMeta {
        last_extended_slot: 0,
        ..lookup_table.meta
    };

    AddressLookupTable::overwrite_meta_data(&mut account.data, metadata)?;

    Ok(())
}
