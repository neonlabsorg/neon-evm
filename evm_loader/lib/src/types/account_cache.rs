// use crate::tracing::tracers::state_diff::Account;
use crate::rpc::Rpc;
use async_trait::async_trait;
// use async_trait::async_trait;
use bincode::deserialize;
use futures::future::join_all;
use solana_client::client_error::Result as ClientResult;
use solana_sdk::{
    account::Account,
    bpf_loader_upgradeable::UpgradeableLoaderState,

    // account_utils::StateMut,
    clock::{Slot, UnixTimestamp},
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

fn get_programdata_slot_from_account(acc: &Account) -> u64 {
    //probably will not serrialize.
    match deserialize::<UpgradeableLoaderState>(&acc.data) {
        Ok(UpgradeableLoaderState::ProgramData { slot, .. }) => slot,
        Ok(_) => {
            panic!("Account is not of type `ProgramData`.");
        }
        Err(e) => {
            eprintln!("Error occurred: {e:?}");
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

    assert!(
        programdata_keys.len() == future_requests.len(),
        "programdata_keys.size()!=future_requests.size()"
    );
    let results = join_all(future_requests).await;

    for (i, result) in results.iter().enumerate() {
        let key = programdata_keys[i];
        match result {
            Ok(Some(account)) => {
                // Extract the slot value from the account data
                let slot_val = get_programdata_slot_from_account(account);
                // Assuming `acc_hash_get` is an async function that returns an `Option`
                if let Some(acc) = acc_hash_get(key, slot_val).await {
                    answer.push(Some(acc));
                } else if let Ok(Some(tmp_acc)) = rpc.get_account(&key).await {
                    acc_hash_add(key, slot_val, tmp_acc.clone()).await;
                    answer.push(Some(tmp_acc));
                } else {
                    answer.push(None);
                }
            }
            Ok(None) => {
                println!("Account for key {key:?} is None.");
                // need return
            }
            Err(e) => {
                println!("Error fetching account for key {key:?}: {e:?}");
            }
        }
    }
    // let mut answer_arr=Vec::new();
    Ok(answer)
}

struct FakeRpc {
    accounts: HashMap<Pubkey, Account>,
    my_pubkey: Pubkey,
}
#[allow(dead_code)]
impl FakeRpc {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            my_pubkey: Pubkey::new_unique(),
        }
    }

    fn has_account(&self, pubkey: &Pubkey) -> bool {
        self.accounts.contains_key(pubkey)
    }

    fn make_account(&mut self, pubkey: Pubkey) -> Account {
        let answer = Account::new(1, 4 * 1024 * 1024, &self.my_pubkey);

        self.accounts.insert(pubkey, answer.clone());
        answer
    }
}

#[async_trait(?Send)]

impl Rpc for FakeRpc {
    async fn get_account(&self, pubkey: &Pubkey) -> ClientResult<Option<Account>> {
        assert!(self.accounts.contains_key(pubkey), "  ");
        Ok(Some(self.accounts.get(pubkey).unwrap().clone()))
    }

    async fn get_account_slice(
        &self,
        pubkey: &Pubkey,
        offset: usize,
        data_size: usize,
    ) -> ClientResult<Option<Account>> {
        assert!(self.accounts.contains_key(pubkey), "  ");
        let mut answer = self.accounts.get(pubkey).unwrap().clone();
        if offset != 0 {
            answer
                .data
                .drain(..std::cmp::min(answer.data.len(), offset));
        }
        answer.data.truncate(data_size);

        Ok(Some(answer))
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        let mut futures = Vec::new();
        for pubkey in pubkeys {
            futures.push(self.get_account(pubkey).await?);
        }

        Ok(futures)
    }

    async fn get_block_time(&self, _slot: Slot) -> ClientResult<UnixTimestamp> {
        Ok(9999)
    }
    async fn get_slot(&self) -> ClientResult<Slot> {
        Ok(1212)
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        Ok(Vec::new())
    }
}
use tokio;
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acc_is_exist() {
        let mut rpc = FakeRpc::new();
        let key1 = Pubkey::new_unique();

        let test_acc = rpc.make_account(key1);
        if test_acc.data.len() >= 4000000 {
            println!("Account data len: {}", test_acc.data.len());
        } else {
            panic!("test stop");
        }
        if let Ok(test2_acc) = rpc.get_account(&key1).await {
            assert_eq!(
                test_acc.data.len(),
                test2_acc.expect("test fail").data.len()
            );
        } else {
            panic!("fake rpc returned error");
        }

        let test3_acc = rpc.get_account_slice(&key1, 0, 1024).await;
        assert_eq!(1024, test3_acc.unwrap().expect("test fail").data.len());
    }
}
