use solana_sdk::{
    account::Account,
    pubkey::Pubkey,
};
use clickhouse::{Client as DBClient, error::Error};
use log::{debug};
use tokio::task::block_in_place;

use serde::{Serialize, Deserialize };

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct DBConfig{
    pub url: String,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, serde::Deserialize, clickhouse::Row, Clone)]
struct AccountRow {
    pubkey: [u8; 32],
    lamports: u64,
    data: Vec<u8>,
    owner: [u8; 32],
    executable: bool,
    rent_epoch: u64,
}

impl From<AccountRow> for Account {
    fn from(row: AccountRow) -> Account {
        Account {
            lamports: row.lamports,
            data: row.data,
            owner: Pubkey::new_from_array(row.owner),
            executable: row.executable,
            rent_epoch: row.rent_epoch,
        }
    }
}


pub struct ClickHouseClient {
    client: DBClient,
    pub slot: u64,
}


impl ClickHouseClient {
    #[allow(unused)]
    pub fn new(config: &DBConfig, slot: u64) -> Self {
        let client = DBClient::default()
            .with_url(config.url.clone())
            .with_user(config.user.clone())
            .with_password(config.password.clone())
            .with_database(config.database.clone());

        ClickHouseClient { client, slot }
    }

    fn block<F, Fu, R>(&self, f: F) -> R
        where
            F: FnOnce(DBClient) -> Fu,
            Fu: std::future::Future<Output = R>,
    {
        let client = self.client.clone();
        block_in_place(|| {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(f(client))
        })
    }

    #[tokio::main]
    pub async  fn get_accounts_at_slot(
        &self,
        pubkeys: impl Iterator<Item = Pubkey>,
    ) -> Result<Vec<(Pubkey, Account)>, Error> {
        let pubkeys = pubkeys
            .map(|pubkey| hex::encode(&pubkey.to_bytes()[..]))
            .fold(String::new(), |old, addr| {
                format!("{} unhex('{}'),", old, addr)
            });


        let accounts = self.block(|client| async move {
            client
                .query(&format!(
                    "SELECT
                        public_key,
                        argMax(lamports, T.slot),
                        argMax(data, T.slot),
                        argMax(owner,T.slot),
                        argMax(executable,T.slot),
                        argMax(rent_epoch,T.slot)
                     FROM accounts A
                     JOIN transactions T
                     ON A.transaction_signature = T.transaction_signature
                     WHERE T.slot <= ? AND public_key IN ({})
                     GROUP BY public_key",
                     pubkeys
                ))
                .bind(self.slot)
                .fetch_all::<AccountRow>()
                .await
        })?;


        let accounts = accounts
            .into_iter()
            .map(|row| (Pubkey::new_from_array(row.pubkey), Account::from(row)))
            .collect();
        debug!("found account: {:?}", accounts);
        Ok(accounts)
    }

    pub fn get_account_at_slot(&self, pubkey: &Pubkey) -> Result<Option<Account>, Error> {
        let accounts = self.get_accounts_at_slot(std::iter::once(*pubkey))?;
        let account = accounts.get(0).map(|(_, account)| account).cloned();
        Ok(account)
    }

}
