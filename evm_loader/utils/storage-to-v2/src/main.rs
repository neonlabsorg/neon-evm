#![allow(deprecated)]

use std::env::current_dir;
use std::fs::File;
use std::io::Write;
use std::mem::size_of;
use std::ops::{Add, Sub};
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use arrayref::array_ref;
use evm_core::{H160, U256};
use evm_loader::account::{ACCOUNT_SEED_VERSION, AccountData, ether_account, ether_contract, ether_storage, Packable};
use evm_loader::config::STORAGE_ENTIRIES_IN_CONTRACT_ACCOUNT;
use evm_loader::hamt::Hamt;
use rustc_hash::FxHashMap;
use serde_json::{json, Value};
use solana_account_decoder::{UiAccountEncoding, UiDataSliceConfig};
use solana_client::client_error::Result as ClientResult;
use solana_client::rpc_client::{RpcClient, serialize_and_encode};
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter;
use solana_client::rpc_filter::{MemcmpEncodedBytes, RpcFilterType};
use solana_program::account_info::AccountInfo;
use solana_program::hash::Hash;
use solana_program::pubkey;
use solana_sdk::account::{Account, ReadableAccount};
use solana_sdk::account_info::IntoAccountInfo;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::keypair::{Keypair, read_keypair_file};
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_sdk::transaction::Transaction;
use solana_transaction_status::UiTransactionEncoding;

use crate::json_rpc::{JsonRpcClient, Request};

mod json_rpc;

macro_rules! print {
    ($($arg:tt)*) => {
        std::print!($($arg)*);
        std::io::stdout().flush().unwrap();
    }
}

macro_rules! println {
    () => (print!("\n"));
    ($($arg:tt)*) => ({
        std::println!($($arg)*);
        std::io::stdout().flush().unwrap();
    })
}

#[derive(serde_derive::Deserialize)]
struct Config {
    url: String,
    evm_loader_pubkey: String,
    batch_size: usize,
    recent_block_hash_ttl_sec: u64,
    client_timeout_sec: u64,
    show_errors: bool,
    skip_preflight: bool,
    max_tps: usize,
    skip_backup: bool,
}

struct ContractV1<'a> {
    ether_address: H160,
    owner: &'a Pubkey,
    storage: &'a Hamt<'a>,
}

type EthereumContractV1<'a> = AccountData<'a, ether_contract::DataV1, ether_contract::ExtensionV1<'a>>;
type ContractsV1Map<'a> = FxHashMap<&'a Pubkey, ContractV1<'a>>;
type EtherAddressesMap = FxHashMap<Pubkey, H160>;
type DataWrittenMap = FxHashMap<Pubkey, U256>;

lazy_static::lazy_static! {
    static ref CONFIG: Config = serde_json::from_reader(
        File::open("config.json").expect("Failed to open `config.json` file"),
    ).expect("Failed to parse configuration file");
    static ref EVM_LOADER: Pubkey = Pubkey::from_str(&CONFIG.evm_loader_pubkey)
        .expect("Failed to parse `evm_loader_pubkey` in config");
    static ref PAYER: Keypair = read_keypair_file("payer.keys.json")
        .expect("Failed to read `payer.keys.json` file");
    static ref EXCLUDE_V1_CONTRACTS: Vec<Pubkey> = vec![
        pubkey!("74gQvu6R5DnSFdJ9JoMXFzk3e7uZgo9cZKxrdZBW8RaH"),
        pubkey!("9HYmDSLt1svoJB23CkEZ9iMUCRUoNVj7iUS7T6pHPYr5"),
    ];
}

struct RecentBlockHash<'a> {
    client: &'a RpcClient,
    hash: Hash,
    time: Instant,
    recent_block_hash_ttl_sec: u64,
}

impl <'a> RecentBlockHash<'a> {
    fn new(client: &'a RpcClient, recent_block_hash_ttl_sec: u64) -> Self {
        Self {
            client,
            hash: Hash::new_from_array([0; 32]),
            time: Instant::now().sub(Duration::from_secs(60 * 60 * 24)),
            recent_block_hash_ttl_sec,
        }
    }

    fn get(&mut self) -> &Hash {
        if Instant::now().duration_since(self.time).as_secs() > self.recent_block_hash_ttl_sec {
            match self.client.get_latest_blockhash_with_commitment(CommitmentConfig::confirmed()) {
                Ok((hash, _)) => {
                    self.hash = hash;
                    self.time = Instant::now();
                    println!("New recent block hash: {}", self.hash);
                },
                Err(err) =>
                    println!("Failed to get recent blockhash: {:?}, using old value: {}", err, self.hash),
            }
        }

        &self.hash
    }
}

struct Batch<'url> {
    client: JsonRpcClient<'url>,
    batch: Vec<Request>,
    batch_size: usize,
    show_errors: bool,
    skip_preflight: bool,
    max_tps: usize,
    created_at: Instant,
    transaction_count: usize,
}

impl<'url> Batch<'url> {
    pub fn new(
        client: JsonRpcClient<'url>,
        batch_size: usize,
        show_errors: bool,
        skip_preflight: bool,
        max_tps: usize,
    ) -> Self {
        Self {
            client,
            batch: Vec::with_capacity(batch_size),
            batch_size,
            show_errors,
            skip_preflight,
            max_tps,
            created_at: Instant::now(),
            transaction_count: 0,
        }
    }

    pub fn send(&mut self) {
        if self.batch.is_empty() {
            return;
        }

        let next_transaction_at = self.created_at.add(
            Duration::from_secs_f64(self.transaction_count as f64 / self.max_tps as f64)
        );
        while next_transaction_at > Instant::now() {
            sleep(Duration::from_millis(10));
        }

        let now = Instant::now();
        if now - self.created_at > Duration::from_secs(20) {
            self.created_at = now;
            self.transaction_count = 0;
        }

        print!("Sending batch of {} requests... ", self.batch.len());
        if self.show_errors {
            println!();
        }
        match self.client.send_batch(&self.batch) {
            Ok(Value::Array(responses)) => {
                let mut error_count = 0;
                for response in responses {
                    if let Value::String(ref error_message) = response["error"]["message"] {
                        if self.show_errors {
                            println!("Error: {}", error_message);
                        }
                        error_count += 1;
                    }
                }
                if error_count == 0 {
                    println!("OK")
                } else {
                    println!("{} error(s)", error_count);
                }
            }
            Ok(response) => println!("Error: {:?}", response),
            Err(error) => println!("Error: {:?}", error),
        }

        self.transaction_count += self.batch.len();
        self.batch.clear();
    }

    pub fn add(&mut self, transaction: &Transaction) {
        let serialized = serialize_and_encode(transaction, UiTransactionEncoding::Base64)
            .expect("Transaction serialization error");
        let request = self.client.request(
            "sendTransaction",
            json!([
                serialized,
                {
                    "skipPreflight": self.skip_preflight,
                    "preflightCommitment": "confirmed",
                    "encoding": "base64",
                },
            ])
        );
        self.batch.push(request);
        if self.batch.len() >= self.batch_size {
            self.send();
        }
    }
}

fn write_value_instruction(
    ether_account: Pubkey,
    storage_address: Pubkey,
    key: U256,
    value: U256,
) -> Instruction {
    let mut data = vec![28_u8; 1 + 32 + 32];
    key.to_big_endian(&mut data[1..33]);
    value.to_big_endian(&mut data[33..]);

    Instruction::new_with_bytes(
        *EVM_LOADER,
        &data,
        vec![
            AccountMeta::new_readonly(PAYER.pubkey(), true),         // Operator
            AccountMeta::new_readonly(system_program::id(), false),  // System program
            AccountMeta::new_readonly(ether_account, false),         // Ether account
            AccountMeta::new(storage_address, false),                // Storage account
        ],
    )
}

fn convert_to_v2_instruction(
    ether_account: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        *EVM_LOADER,
        &[29u8],
        vec![
            AccountMeta::new_readonly(PAYER.pubkey(), true),         // Funding account
            AccountMeta::new_readonly(system_program::id(), false),  // System program
            AccountMeta::new(ether_account, false),                  // Ether account
        ],
    )
}

fn get_storage_address(address: &H160, index: &U256) -> Pubkey {
    let mut index_bytes = [0_u8; 32];
    index.to_little_endian(&mut index_bytes);

    let seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], b"ContractStorage", address.as_bytes(), &[0; size_of::<u32>()], &index_bytes];

    Pubkey::find_program_address(seeds, &EVM_LOADER).0
}

fn get_evm_accounts(
    client: &RpcClient,
    tag: u8,
    data_slice: Option<UiDataSliceConfig>,
) -> ClientResult<Vec<(Pubkey, Account)>> {
    client.get_program_accounts_with_config(
        &EVM_LOADER,
        RpcProgramAccountsConfig {
            filters: Some(
                vec![
                    RpcFilterType::Memcmp(
                        rpc_filter::Memcmp {
                            offset: 0,
                            bytes: MemcmpEncodedBytes::Bytes(vec![tag]),
                            encoding: None,
                        }
                    ),
                ]
            ),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                data_slice,
                ..Default::default()
            },
            ..Default::default()
        },
    )
}

fn copy_data_to_distributed_storage<'a>(
    batch: &mut Batch,
    ethereum_contract_v1: &ContractV1<'a>,
    data_written_map: &DataWrittenMap,
    recent_blockhash: &mut RecentBlockHash,
) -> usize {
    let mut count = 0;
    let storage_entries_in_contract_account = U256::from(STORAGE_ENTIRIES_IN_CONTRACT_ACCOUNT);
    for (key, value) in ethereum_contract_v1.storage.iter() {
        if key < storage_entries_in_contract_account || value.is_zero() {
            continue;
        }

        let storage_address = get_storage_address(&ethereum_contract_v1.ether_address, &key);
        if let Some(stored_value) = data_written_map.get(&storage_address) {
            if stored_value == &value {
                continue;
            }
            unreachable!("Something went wrong! {} != {}", value, stored_value);
        }

        let instructions = vec![
            write_value_instruction(*ethereum_contract_v1.owner, storage_address, key, value),
        ];
        let blockhash = recent_blockhash.get();
        let mut message = Message::new(&instructions, Some(&PAYER.pubkey()));
        message.recent_blockhash = *blockhash;
        let mut transaction = Transaction::new_unsigned(message);
        transaction.sign(&[&*PAYER], *blockhash);

        batch.add(&transaction);
        count += 1;
    }

    count
}

fn is_all_data_written(
    data_written_map: &DataWrittenMap,
    ethereum_contract_v1: &ContractV1,
) -> bool {
    let storage_entries_in_contract_account = U256::from(STORAGE_ENTIRIES_IN_CONTRACT_ACCOUNT);
    for (key, value) in ethereum_contract_v1.storage.iter() {
        if key < storage_entries_in_contract_account || value.is_zero() {
            continue;
        }
        let solana_address = get_storage_address(&ethereum_contract_v1.ether_address, &key);
        let stored_value = match data_written_map.get(&solana_address) {
            Some(value) => value,
            None => return false,
        };

        if stored_value != &value {
            panic!(
                "Value of a storage account {} is incorrect. Expected {}, but actual is {}",
                solana_address,
                value,
                stored_value,
            );
        }
    }

    true
}

fn extract_data_to_distributed_storage(
    batch: &mut Batch,
    recent_block_hash: &mut RecentBlockHash,
    contracts_v1_map: &ContractsV1Map,
    data_written_map: &DataWrittenMap,
    mut sent: usize,
) -> Result<()> {
    for ethereum_contract_v1 in contracts_v1_map.values() {
        let count = copy_data_to_distributed_storage(
            batch,
            ethereum_contract_v1,
            data_written_map,
            recent_block_hash,
        );
        sent += count;
        if count > 0 {
            println!("{} value(s) sent (+{})", sent, count);
        }
    }

    Ok(())
}

fn make_convert_to_v2_transaction(pubkey: Pubkey, recent_blockhash: &Hash) -> Transaction {
    let instructions = vec![
        convert_to_v2_instruction(pubkey),
    ];
    let mut message = Message::new(&instructions, Some(&PAYER.pubkey()));
    message.recent_blockhash = *recent_blockhash;
    let mut transaction = Transaction::new_unsigned(message);
    transaction.sign(&[&*PAYER], *recent_blockhash);

    transaction
}

fn convert_accounts_to_v2(
    batch: &mut Batch,
    recent_block_hash: &mut RecentBlockHash,
    contracts_v1_map: &ContractsV1Map,
    data_written_map: &DataWrittenMap,
) -> Result<()> {
    for (pubkey, ethereum_contract_v1) in contracts_v1_map.iter() {
        if is_all_data_written(data_written_map, ethereum_contract_v1) {
            batch.add(
                &make_convert_to_v2_transaction(
                    **pubkey,
                    recent_block_hash.get(),
                ),
            );
        }
    }

    Ok(())
}

fn obtain_ether_addresses_map(client: &RpcClient) -> ClientResult<EtherAddressesMap> {
    fn get_addresses<T: FromIterator<(Pubkey, H160)>>(
        client: &RpcClient,
        tag: u8,
    ) -> ClientResult<T> {
        get_evm_accounts(
            client,
            tag,
            Some(UiDataSliceConfig { offset: 1, length: size_of::<H160>() }),
        ).map(|vec| vec.into_iter()
            .map(|(pubkey, ref account)|
                (pubkey, H160::from(array_ref!(account.data(), 0, 20)))
            ).collect()
        )
    }

    let /*mut*/ addresses: EtherAddressesMap = get_addresses(client, ether_account::Data::TAG)?;
    // We decided to ignore V1 accounts:
    // addresses.extend(get_addresses::<Vec<(Pubkey, H160)>>(client, ether_account::DataV1::TAG)?);

    Ok(addresses)
}

fn obtain_data_written_map(client: &RpcClient) -> ClientResult<DataWrittenMap> {
    get_evm_accounts(
        client,
        ether_storage::Data::TAG,
        Some(UiDataSliceConfig { offset: 1, length: size_of::<U256>() }),
    ).map(|vec| vec.into_iter()
        .map(|(pubkey, account)|
            (pubkey, U256::from_big_endian_fast(&account.data[..]))
        )
        .collect()
    )
}

fn count_storage_accounts(contracts_v1_map: &ContractsV1Map) -> usize {
    let storage_entries_in_contract_account = U256::from(STORAGE_ENTIRIES_IN_CONTRACT_ACCOUNT);
    contracts_v1_map.iter()
        .map(|(pubkey, ether_contract)| {
            ether_contract.storage.iter()
                .on_error(|_err| println!("Data corrupted in HAMT for account: {}", pubkey))
                .filter(|(key, value)| *key >= storage_entries_in_contract_account && !value.is_zero())
                .count()
        }
    ).sum()
}

fn main() -> Result<()> {
    println!("Payer public key: {}", PAYER.pubkey());

    let client = RpcClient::new_with_timeout(
        &CONFIG.url,
        Duration::from_secs(CONFIG.client_timeout_sec),
    );

    print!("Querying accounts for Ethereum addresses map... ");
    let mut ether_addresses_map = obtain_ether_addresses_map(&client)?;
    println!("OK ({} accounts)", ether_addresses_map.len());

    print!("Querying Contract V1 accounts... ");
    let mut contract_v1_accounts = get_evm_accounts(&client, ether_contract::DataV1::TAG, None)?;
    print!("Queried {} accounts. ", contract_v1_accounts.len());

    if !CONFIG.skip_backup {
        let path = current_dir()?
            .join("backups")
            .join(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs()
                    .to_string(),
            );
        std::fs::create_dir_all(&path)?;
        print!("Backing up to {:?}... ", path);
        for (pubkey, account_info) in contract_v1_accounts.iter() {
            std::fs::write(
                path.join(pubkey.to_string()),
                account_info.data(),
            )?;
        }
    }

    print!("Transforming...");

    let contracts_v1_info: Vec<AccountInfo> = contract_v1_accounts.iter_mut()
        .map(|(pubkey, account)| (&*pubkey, account).into_account_info())
        .collect();

    let ether_contracts_v1: Vec<(&Pubkey, EthereumContractV1)> = contracts_v1_info.iter()
        .map(|info| {
            (
                info.key,
                EthereumContractV1::from_account(&EVM_LOADER, info)
                    .unwrap_or_else(|err|
                        panic!("Cannot decode contract V1 data for account: {}, error: {:?}", info.key, err)
                    )
            )
        }).collect();

    let mut contracts_v1_map: ContractsV1Map = ether_contracts_v1.iter()
        .filter_map(|(pubkey, contract_v1)| {
            let ether_address = ether_addresses_map.remove(&contract_v1.owner)?;
                // We decided to ignore V1 accounts:
                // .expect(&format!("Ethereum address not found for Solana account: {}", contract_v1.owner));
            Some((
                *pubkey,
                ContractV1 {
                    ether_address,
                    owner: &contract_v1.owner,
                    storage: &contract_v1.extension.storage,
                },
            ))
        })
        .collect();
    drop(ether_addresses_map);

    for exclude_pubkey in EXCLUDE_V1_CONTRACTS.iter() {
        contracts_v1_map.remove(exclude_pubkey);
    }
    println!("OK ({} accounts)", contracts_v1_map.len());

    print!("Counting expected infinite storage accounts to create... ");
    let expected_storage_accounts_count = count_storage_accounts(&contracts_v1_map);
    println!("{} accounts", expected_storage_accounts_count);

    let mut recent_block_hash = RecentBlockHash::new(&client, CONFIG.recent_block_hash_ttl_sec);
    loop {
        print!("Querying already written infinite storage accounts... ");
        let data_written_map = obtain_data_written_map(&client)?;
        println!("OK ({} values)", data_written_map.len());
        println!("Accounts to convert: {}", contracts_v1_map.len());

        let mut batch = Batch::new(
            JsonRpcClient::new(&CONFIG.url),
            CONFIG.batch_size,
            CONFIG.show_errors,
            CONFIG.skip_preflight,
            CONFIG.max_tps,
        );

        println!("Converting accounts from V1 to V2...");

        convert_accounts_to_v2(
            &mut batch,
            &mut recent_block_hash,
            &contracts_v1_map,
            &data_written_map,
        )?;

        batch.send();

        print!("Querying converted storage accounts... ");
        let contracts_v2 = get_evm_accounts(
            &client,
            ether_contract::Data::TAG,
            Some(UiDataSliceConfig { offset: 0, length: 0 }),
        )?;
        println!("OK ({} accounts)", contracts_v2.len());

        print!("Removing converted accounts... ");
        let mut removed = 0;
        for (pubkey, _account) in contracts_v2 {
            if contracts_v1_map.remove(&pubkey).is_some() {
                removed += 1;
            }
        }
        println!("{} accounts removed", removed);

        if contracts_v1_map.is_empty() {
            return Ok(());
        }

        println!("Extracting data to distributed storage...");

        print!("Counting expected infinite storage accounts to create... ");
        let expected_storage_accounts_count = count_storage_accounts(&contracts_v1_map);
        println!("{} accounts", expected_storage_accounts_count);

        extract_data_to_distributed_storage(
            &mut batch,
            &mut recent_block_hash,
            &contracts_v1_map,
            &data_written_map,
            data_written_map.len(),
        )?;

        batch.send();
    }
}
