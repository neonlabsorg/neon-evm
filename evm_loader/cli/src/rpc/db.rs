use solana_sdk::{
    account::Account,
    pubkey::Pubkey,
};
use tokio_postgres::{Client as DBClient, connect, Error};
use postgres::{ NoTls };
// use tokio::task::block_in_place;
use serde::{Serialize, Deserialize };

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct DBConfig{
    pub host: String,
    pub port: String,
    pub database: String,
    pub user: String,
    pub password: String,
}

pub struct PostgresClient {
    client: DBClient,
    pub slot: u64,
}


impl PostgresClient {
    // #[allow(unused)]
    #[tokio::main]
    pub async fn new(config: &DBConfig, slot: u64) -> Self {
        let connection_str= format!("host={} port={} dbname={} user={} password={}",
                                    config.host, config.port, config.database, config.user, config.password);

        let (client, connection) =
            connect(&connection_str, NoTls).await.unwrap();

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });
        Self {client, slot}
    }

    // fn block<F, Fu, R>(&self, f: F) -> R
    //     where
    //         F: FnOnce(DBClient) -> Fu,
    //         Fu: std::future::Future<Output = R>,
    // {
    //     let client = self.client.clone();
    //     block_in_place(|| {
    //         let handle = tokio::runtime::Handle::current();
    //         handle.block_on(f(client))
    //     })
    // }

    #[tokio::main]
    pub async fn get_accounts_at_slot(&self, keys: impl Iterator<Item = Pubkey>) -> Result<Vec<(Pubkey, Account)>, Error> {
        let key_bytes = keys.map(|entry| entry.to_bytes()).collect::<Vec<_>>();
        let key_slices = key_bytes.iter().map(|entry| entry.as_slice()).collect::<Vec<_>>();

        let mut result = vec![];

        // let rows = self.block(|| async {
        //     self.client.query(
        //         "SELECT * FROM get_accounts_at_slot($1, $2)",&[&key_slices, &(self.slot as i64)]
        //     ).await
        // })?;

        let rows = self.client.query(
            "SELECT * FROM get_accounts_at_slot($1, $2)",&[&key_slices, &(self.slot as i64)]
        ).await?;

        for row in rows {
            let lamports: i64 = row.try_get(2)?;
            let rent_epoch: i64 = row.try_get(4)?;
            result.push((
                Pubkey::new(row.try_get(0)?),
                Account {
                    lamports: lamports as u64,
                    data: row.try_get(5)?,
                    owner: Pubkey::new(row.try_get(1)?),
                    executable: row.try_get(3)?,
                    rent_epoch: rent_epoch as u64,
                }
            ));
        }
        Ok(result)
    }

    pub fn get_account_at_slot(&self, pubkey: &Pubkey) -> Result<Option<Account>, Error> {
        let accounts = self.get_accounts_at_slot(std::iter::once(*pubkey))?;
        let account = accounts.get(0).map(|(_, account)| account).cloned();
        Ok(account)
    }
}
