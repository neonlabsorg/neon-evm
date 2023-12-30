use solana_sdk::account::AccountSharedData;
use solana_sdk::rent::Rent;
use solana_sdk::bpf_loader_upgradeable;
use std::collections::BTreeMap;
use std::cell::RefCell;

use crate::syscall_stubs;
use crate::NeonError;
use evm_loader::{
    account_storage::AccountStorage,
    executor::OwnedAccountInfo,
};
use solana_sdk::{account::Account, account_info::AccountInfo, pubkey, pubkey::Pubkey, instruction::{AccountMeta, Instruction}};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};

const SEEDS_PUBKEY: Pubkey = pubkey!("Seeds11111111111111111111111111111111111111");

pub struct SolanaEmulator {
    program_id: Pubkey,
    emulator_context: RefCell<ProgramTestContext>,
}

macro_rules! processor_with_original_stubs {
    ($process_instruction:expr) => {
        processor!(|program_id, accounts, instruction_data| {
            let use_original_stubs_saved = syscall_stubs::use_original_stubs_for_thread(true);
            let result = $process_instruction(program_id, accounts, instruction_data);
            syscall_stubs::use_original_stubs_for_thread(use_original_stubs_saved);
            result
        })
    };
}

// evm_loader stub to call solana programs like from original program
// Pass signer seeds through the special account's data.
fn process_emulator_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> solana_sdk::entrypoint::ProgramResult {
    use solana_sdk::program_error::ProgramError;

    let seeds: Vec<Vec<u8>> = bincode::deserialize(&accounts[0].data.borrow())
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let seeds = seeds.iter()
        .map(|v| v.as_slice())
        .collect::<Vec<&[u8]>>();
    let signer = Pubkey::create_program_address(&seeds, program_id)
        .map_err(|_| ProgramError::InvalidSeeds)?;

    let instruction = Instruction::new_with_bytes(
        *accounts[1].key,
        instruction_data,
        accounts[2..].iter().map(|a| {
            AccountMeta {
                pubkey: *a.key, 
                is_signer: if *a.key == signer {true} else {a.is_signer}, 
                is_writable: a.is_writable
            }
        }).collect::<Vec<_>>(),
    );

    solana_sdk::program::invoke_signed_unchecked(
        &instruction, 
        accounts, 
        &[&seeds]
    )
}

impl SolanaEmulator {
    pub async fn new(
        program_id: Pubkey,
    ) -> Result<SolanaEmulator, NeonError> {
        let mut program_test = ProgramTest::default();
        program_test.prefer_bpf(false);
        program_test.add_program(
            "evm_loader",
            program_id,
            processor_with_original_stubs!(process_emulator_instruction),
        );

        // TODO: disable features (get known feature list and disable by actual value)
        let emulator_context = program_test.start_with_context().await;

        Ok(Self {
            program_id,
            emulator_context: RefCell::new(emulator_context),
        })
    }

    pub async fn emulate_solana_call<B: AccountStorage>(
        &self,
        backend: &B,
        instruction: &Instruction,
        // program_id: &Pubkey,
        // instruction_data: &[u8],
        // meta: &[AccountMeta],
        accounts: &mut BTreeMap<Pubkey, OwnedAccountInfo>,
        seeds: &Vec<Vec<u8>>,
    ) -> evm_loader::error::Result<()> {
        use solana_sdk::signature::Signer;
        //use std::collections::btree_map::Entry;
        use bpf_loader_upgradeable::UpgradeableLoaderState;

        let mut emulator_context = self.emulator_context.borrow_mut();

        // async fn get_cached_or_create_account(
        //     key: &Pubkey,
        //     accounts: &mut BTreeMap<Pubkey, OwnedAccountInfo>,
        //     storage: &EmulatorAccountStorage<'_>,
        // ) -> OwnedAccountInfo {
        //     let entry = accounts.entry(*key);
        //     match entry {
        //         Entry::Occupied(entry) => {
        //             entry.get().clone()
        //         }
        //         Entry::Vacant(entry) => {
        //             let account = storage.clone_solana_account(entry.key()).await;
        //             entry.insert(account).clone()
        //         }
        //     }
        // }

        let mut append_account_to_emulator = |account: &OwnedAccountInfo| {
            use solana_sdk::account::WritableAccount;
            let mut shared_account = AccountSharedData::new(account.lamports, account.data.len(), &account.owner);
            shared_account.set_data_from_slice(&account.data);
            shared_account.set_executable(account.executable);
            emulator_context.set_account(&account.key, &shared_account);
        };

        for (index, m) in instruction.accounts.iter().enumerate() {
            //let account = get_cached_or_create_account(&m.pubkey, accounts, self).await;
            let account = accounts.get(&m.pubkey).expect("Missing pubkey in accounts map");
            append_account_to_emulator(account);
            log::debug!("{} {}: {:?}", index, m.pubkey, to_account(&account));
        }

        //let program = get_cached_or_create_account(&program_id, accounts, self).await;
        let program = match accounts.get(&instruction.program_id) {
            Some(&ref account) => account.clone(),
            None => backend.clone_solana_account(&instruction.program_id).await,
        };
        append_account_to_emulator(&program);
        log::debug!("program {}: {:?}", instruction.program_id, to_account(&program));

        if bpf_loader_upgradeable::check_id(&program.owner) {
            if let UpgradeableLoaderState::Program{programdata_address} = bincode::deserialize(program.data.as_slice()).unwrap() {
                //let program_data = get_cached_or_create_account(&programdata_address, accounts, self).await;
                let program_data = match accounts.get(&programdata_address) {
                    Some(&ref account) => account.clone(),
                    None => backend.clone_solana_account(&programdata_address).await,
                };
                append_account_to_emulator(&program_data);
                log::debug!("programData {}: {:?}", programdata_address, to_account(&program_data));
            };
        }

        let seed = seeds.iter().map(|s| s.as_ref()).collect::<Vec<&[u8]>>();
        let seeds_data = bincode::serialize(&seeds).expect("Serialize seeds");
        append_account_to_emulator(&OwnedAccountInfo {
            key: SEEDS_PUBKEY,
            is_signer: false,
            is_writable: false,
            lamports: Rent::default().minimum_balance(seeds_data.len()),
            data: seeds_data,
            owner: self.program_id,
            executable: false,
            rent_epoch: 0,
        });

        let mut accounts_meta = vec!(
            AccountMeta {pubkey: SEEDS_PUBKEY, is_signer: false, is_writable: false,},
            AccountMeta {pubkey: instruction.program_id, is_signer: false, is_writable: false,},
        );
        let invoke_signer = Pubkey::create_program_address(&seed, &self.program_id)
            .expect("Create invoke_signer from seeds");
        accounts_meta.extend(instruction.accounts.iter().map(|m| {
            AccountMeta {
                pubkey: m.pubkey,
                is_signer: if m.pubkey == invoke_signer { false } else { m.is_signer },
                is_writable: m.is_writable,
            }
        }));

        // Prepare transaction to execute on emulator
        let mut trx = solana_sdk::transaction::Transaction::new_unsigned(
            solana_sdk::message::Message::new(
                &[
                    solana_sdk::instruction::Instruction::new_with_bytes(
                        self.program_id,
                        &instruction.data,
                        accounts_meta,
                    ),
                ],
                Some(&emulator_context.payer.pubkey()),
            ),
        );
        trx.try_sign(&[&emulator_context.payer], emulator_context.last_blockhash)
            .map_err(|e| evm_loader::error::Error::Custom(e.to_string()))?;

        let result = emulator_context.banks_client.process_transaction(trx).await;
        log::info!("Emulation result: {:?}", result);
        result.map_err(|e| evm_loader::error::Error::Custom(e.to_string()))?;
        let next_slot = emulator_context.banks_client.get_root_slot().await.unwrap() + 1;
        emulator_context.warp_to_slot(next_slot).expect("Warp to next slot");

        // Update writable accounts
        for (index, m) in instruction.accounts.iter().enumerate() {
            if m.is_writable {
                let account = emulator_context
                    .banks_client
                    .get_account(m.pubkey)
                    .await
                    .unwrap()
                    .unwrap_or_default();

                accounts.entry(m.pubkey).and_modify(|a| {
                    log::debug!("{} {}: Modify {:?}", index, m.pubkey, account);
                    a.lamports = account.lamports;
                    a.data = account.data.to_vec();
                    a.owner = account.owner;
                    a.executable = account.executable;
                    a.rent_epoch = account.rent_epoch;
                });
                // }).or_insert_with(|| {
                //     log::debug!("{} {}: Insert {:?}", index, m.pubkey, account);
                //     OwnedAccountInfo {
                //         key: m.pubkey,
                //         is_signer: false,
                //         is_writable: false,
                //         lamports: account.lamports,
                //         data: account.data.to_vec(),
                //         owner: account.owner,
                //         executable: account.executable,
                //         rent_epoch: account.rent_epoch,
                //     }
                //});
            }
        }

        Ok(())
    }
}

// Creates new instance of `Account` from `OwnedAccountInfo`
fn to_account(account: &OwnedAccountInfo) -> Account {
    Account {
        lamports: account.lamports,
        data: account.data.clone(),
        owner: account.owner,
        executable: account.executable,
        rent_epoch: account.rent_epoch,
    }
}