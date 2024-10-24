// use crate::tracing::tracers::state_diff::Account;
use crate::rpc::Rpc;
use bincode::deserialize;
use futures::future::join_all;
use solana_client::client_error::Result as ClientResult;
use solana_sdk::{
    account::Account,
    // account_utils::StateMut,
    bpf_loader_upgradeable::UpgradeableLoaderState,

    pubkey::Pubkey,
};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use tokio::sync::OnceCell;

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct KeyAccountCache {
    addr: Pubkey,
    slot: u64,
}
//
// impl Hash for KeyAccountCache
// {
//     fn hash<H: std::hash::Hasher>(&self, state: &mut H)
//     {
//         self.addr.hash(state);
//         self.slot.hash(state);
//     }
// }
#[allow(dead_code)]
type AccCache = HashMap<KeyAccountCache, Account>;
type ProtectedAppCache = RwLock<AccCache>;
#[allow(dead_code)]
static LOCAL_CONFIG: OnceCell<ProtectedAppCache> = OnceCell::const_new();

#[allow(dead_code)]
async fn acc_hash_get_instance() -> &'static ProtectedAppCache {
    LOCAL_CONFIG
        .get_or_init(|| async {
            let map = HashMap::new();

            RwLock::new(map)
        })
        .await
}

#[allow(dead_code)]
async fn acc_hash_get(addr: Pubkey, slot: u64) -> Option<Account> {
    let val = KeyAccountCache { addr, slot };
    acc_hash_get_instance()
        .await
        .read()
        .expect("acc_hash_get_instance poisoned")
        .get(&val)
        .cloned()
}

#[allow(dead_code)]
async fn acc_hash_add(addr: Pubkey, slot: u64, acc: Account) {
    let val = KeyAccountCache { addr, slot };
    acc_hash_get_instance()
        .await
        .write()
        .expect("PANIC, no nable")
        .insert(val, acc);
}

fn get_programdata_slot_from_account(acc: &Account) -> Option<u64> {
    //probably will not serrialize.
    match deserialize::<UpgradeableLoaderState>(&acc.data) {
        Ok(UpgradeableLoaderState::ProgramData { slot, .. }) => Some(slot),
        Ok(_) => {
            panic!("Account is not of type `ProgramData`.");
        }
        Err(_) => {
            panic!("Failed to deserialize account data.");
        }
    }
}

pub async fn acc_hash_get_values_by_keys(
    programdata_keys: &Vec<Pubkey>,
    rpc: &impl Rpc,
) -> ClientResult<Vec<Option<solana_sdk::account::Account>>> {
    let mut future_requests = Vec::new();
    let mut answer = Vec::new();

    for key in programdata_keys {
        future_requests.push(rpc.get_account_slice(
            key,
            0,
            UpgradeableLoaderState::size_of_programdata_metadata(),
        ));
        //future_requests.push(rpc.get_account_slice(key, 0, 512));
    }

    if programdata_keys.len() != future_requests.len() {
        panic!("programdata_keys.size()!=future_requests.size()");
    }
    let results = join_all(future_requests).await;

    for (i, result) in results.iter().enumerate() {
        let key = programdata_keys[i];
        match result {
            Ok(Some(account)) => {
                // Extract the slot value from the account data
                if let Some(slot_val) = get_programdata_slot_from_account(account) {
                    // Assuming `acc_hash_get` is an async function that returns an `Option`
                    if let Some(acc) = acc_hash_get(key, slot_val).await {
                        answer.push(Some(acc));
                    } else {
                        if let Ok(Some(tmp_acc)) = rpc.get_account(&key).await {
                            acc_hash_add(key, slot_val, tmp_acc.clone()).await;
                            answer.push(Some(tmp_acc));
                        } else {
                            answer.push(None);
                        }
                    }
                } else {
                    panic!("slot is None.");
                }
            }
            Ok(None) => {
                println!("Account for key {:?} is None.", key);
                // need return
            }
            Err(e) => {
                println!("Error fetching account for key {:?}: {:?}", key, e);
            }
        }
    }
    // let mut answer_arr=Vec::new();
    Ok(answer)
}
