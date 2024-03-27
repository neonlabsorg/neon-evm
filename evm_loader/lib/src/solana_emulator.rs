use solana_banks_interface::TransactionMetadata;
use solana_program_test::{
    processor, BanksClientError, BanksTransactionResultWithMetadata, ProgramTest,
    ProgramTestContext,
};
use solana_sdk::{
    account::AccountSharedData,
    account::WritableAccount,
    account::{Account, ReadableAccount},
    account_info::AccountInfo,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    instruction::{AccountMeta, Instruction},
    message::Message,
    program_error::ProgramError,
    pubkey,
    pubkey::Pubkey,
    rent::Rent,
    signature::Signer,
    system_instruction::MAX_PERMITTED_DATA_LENGTH,
    transaction::{Transaction, TransactionError},
};

use log::info;
use maybe_async::maybe_async;
use tokio::sync::{Mutex, MutexGuard, OnceCell};

use crate::rpc::Rpc;
use crate::NeonError;

#[maybe_async(?Send)]
pub trait ProgramCache {
    type Error: std::error::Error;

    async fn get_account(&self, pubkey: Pubkey) -> Result<Option<Account>, Self::Error>;

    async fn get_programdata(
        &self,
        programdata_pubkey: Pubkey,
    ) -> evm_loader::error::Result<Vec<u8>> {
        info!("ProgramData pubkey: {:?}", programdata_pubkey);
        let programdata = self
            .get_account(programdata_pubkey)
            .await
            .map_err(|e| evm_loader::error::Error::Custom(e.to_string()))?
            .ok_or(ProgramError::UninitializedAccount)?;
        if programdata.owner == bpf_loader_upgradeable::ID {
            if let UpgradeableLoaderState::ProgramData { .. } =
                bincode::deserialize(&programdata.data)?
            {
                return Ok(programdata.data
                    [UpgradeableLoaderState::size_of_programdata_metadata()..]
                    .to_vec());
            }
        }
        Err(solana_sdk::program_error::ProgramError::InvalidAccountData.into())
    }
}

/// SolanaEmulator
/// Note:
/// 1. Use global program_stubs variable (new() function changes it inside ProgramTest::start_with_context)
/// 2. Get list of activated features from solana cluster (this list can't be changed after initialization)
pub struct SolanaEmulator {
    pub program_id: Pubkey,
    pub emulator_context: ProgramTestContext,
    pub evm_loader_program: Account,
}

static SOLANA_EMULATOR: OnceCell<Mutex<SolanaEmulator>> = OnceCell::const_new();
const SEEDS_PUBKEY: Pubkey = pubkey!("Seeds11111111111111111111111111111111111111");

pub async fn get_solana_emulator() -> MutexGuard<'static, SolanaEmulator> {
    SOLANA_EMULATOR
        .get()
        .expect("SolanaEmulator is not initialized")
        .lock()
        .await
}

pub async fn init_solana_emulator(
    program_id: Pubkey,
    rpc_client: &impl Rpc,
) -> &'static Mutex<SolanaEmulator> {
    SOLANA_EMULATOR
        .get_or_init(|| async {
            let emulator = SolanaEmulator::new(program_id, rpc_client)
                .await
                .expect("Initialize SolanaEmulator");

            Mutex::new(emulator)
        })
        .await
}

// evm_loader stub to call solana programs like from original program
// Pass signer seeds through the special account's data.
fn process_emulator_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> solana_sdk::entrypoint::ProgramResult {
    let seeds: Vec<Vec<Vec<u8>>> = bincode::deserialize(&accounts[0].data.borrow())
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let seeds = seeds
        .iter()
        .map(|s| s.iter().map(|s| s.as_slice()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let seeds = seeds.iter().map(|s| s.as_slice()).collect::<Vec<_>>();

    let signers = seeds
        .iter()
        .map(|s| {
            Pubkey::create_program_address(s, program_id).map_err(|_| ProgramError::InvalidSeeds)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let instruction = Instruction::new_with_bytes(
        *accounts[1].key,
        instruction_data,
        accounts[2..]
            .iter()
            .map(|a| AccountMeta {
                pubkey: *a.key,
                is_signer: if signers.contains(a.key) {
                    true
                } else {
                    a.is_signer
                },
                is_writable: a.is_writable,
            })
            .collect::<Vec<_>>(),
    );

    solana_sdk::program::invoke_signed_unchecked(&instruction, accounts, &seeds)
}

impl SolanaEmulator {
    pub async fn new(
        program_id: Pubkey,
        rpc_client: &impl Rpc,
    ) -> Result<SolanaEmulator, NeonError> {
        let mut program_test = ProgramTest::default();
        program_test.prefer_bpf(false);
        program_test.add_program(
            "evm_loader",
            program_id,
            processor!(process_emulator_instruction),
        );

        // Disable features (get known feature list and disable by actual value)
        let feature_list = solana_sdk::feature_set::FEATURE_NAMES
            .iter()
            .map(|feature| feature.0)
            .cloned()
            .collect::<Vec<_>>();
        let features = rpc_client.get_multiple_accounts(&feature_list).await?;

        feature_list
            .into_iter()
            .zip(features)
            .filter_map(|(pubkey, account)| {
                let activated = account
                    .and_then(|ref acc| solana_sdk::feature::from_account(acc))
                    .and_then(|v| v.activated_at);
                match activated {
                    Some(_) => None,
                    None => Some(pubkey),
                }
            })
            .for_each(|feature_id| program_test.deactivate_feature(feature_id));

        let mut emulator_context = program_test.start_with_context().await;
        let evm_loader_program = emulator_context
            .banks_client
            .get_account(program_id)
            .await
            .expect("Can't get evm_loader program account")
            .expect("evm_loader program account not found");

        Ok(Self {
            program_id,
            emulator_context,
            evm_loader_program,
        })
    }

    pub fn payer(&self) -> Pubkey {
        self.emulator_context.payer.pubkey()
    }

    async fn set_programdata(
        &mut self,
        program_id: Pubkey,
        programdata_address: Pubkey,
        programdata: &mut Vec<u8>,
    ) -> evm_loader::error::Result<()> {
        if self
            .emulator_context
            .banks_client
            .get_account(program_id)
            .await
            .map_err(|e| evm_loader::error::Error::Custom(e.to_string()))?
            .is_none()
        {
            // Deploy new program
            let mut program_account = AccountSharedData::new_data(
                Rent::default().minimum_balance(programdata.len()),
                &UpgradeableLoaderState::Program {
                    programdata_address,
                },
                &bpf_loader_upgradeable::ID,
            )?;
            program_account.set_executable(true);

            let mut programdata_data = bincode::serialize(&UpgradeableLoaderState::ProgramData {
                slot: 0,
                upgrade_authority_address: Some(self.payer()),
            })?;
            programdata_data.append(programdata);
            let mut programdata_account = AccountSharedData::new(
                Rent::default().minimum_balance(programdata_data.len()),
                programdata_data.len(),
                &bpf_loader_upgradeable::ID,
            );
            programdata_account.set_data(programdata_data);

            self.emulator_context
                .set_account(&program_id, &program_account);
            self.emulator_context
                .set_account(&programdata_address, &programdata_account);
        } else {
            // Upgrade program
            // let mut program_buffer = bincode::serialize(
            //     &UpgradeableLoaderState::Buffer {
            //         authority_address: Some(self.get_pubkey())
            //     }
            // )?;
            // program_buffer.append(programdata);
            // self.emulator_context.set_account(&BUFFER_PUBKEY, &Account {
            //     lamports: Rent::default().minimum_balance(program_buffer.len()),
            //     data: program_buffer,
            //     owner: solana_sdk::bpf_loader_upgradeable::ID,
            //     executable: false,
            //     rent_epoch: 0,
            // }.into());

            // self.process_transaction(&[
            //     bpf_loader_upgradeable::upgrade(
            //         &program_id,
            //         &BUFFER_PUBKEY,
            //         &self.get_pubkey(),
            //         &self.get_pubkey(),
            //     ),
            // ]).await?;
        }
        Ok(())
    }

    async fn set_account<'a, B: ProgramCache>(
        &mut self,
        program_cache: &B,
        pubkey: &Pubkey,
        account: &AccountSharedData,
    ) -> evm_loader::error::Result<()> {
        if *pubkey == self.payer() {
            return Err(evm_loader::error::Error::InvalidAccountForCall(*pubkey));
        }

        if solana_sdk::bpf_loader_upgradeable::check_id(account.owner()) {
            if let UpgradeableLoaderState::Program {
                programdata_address,
            } = account
                .deserialize_data()
                .map_err(|_| evm_loader::error::Error::AccountInvalidData(*pubkey))?
            {
                let mut programdata = program_cache.get_programdata(programdata_address).await?;
                self.set_programdata(*pubkey, programdata_address, &mut programdata)
                    .await?;
                info!("set_programdata: {:?}", pubkey);
                return Ok(());
            }
        }

        self.emulator_context.set_account(pubkey, account);
        Ok(())
    }

    async fn prepare_transaction(
        &mut self,
        instructions: &[Instruction],
    ) -> evm_loader::error::Result<Transaction> {
        self.emulator_context
            .get_new_latest_blockhash()
            .await
            .map_err(|e| evm_loader::error::Error::Custom(e.to_string()))?;

        let mut trx = Transaction::new_unsigned(Message::new(instructions, Some(&self.payer())));

        trx.try_sign(
            &[&self.emulator_context.payer],
            self.emulator_context.last_blockhash,
        )
        .map_err(|e| evm_loader::error::Error::Custom(e.to_string()))?;

        Ok(trx)
    }

    pub async fn emulate_solana_call<'a, B: ProgramCache>(
        &mut self,
        program_cache: &B,
        instruction: &Instruction,
        accounts: &[AccountInfo<'a>],
        seeds: &[Vec<Vec<u8>>],
    ) -> evm_loader::error::Result<Option<TransactionMetadata>> {
        for account in accounts {
            self.set_account(program_cache, account.key, &from_account_info(account))
                .await?;
        }

        let signers = seeds
            .iter()
            .map(|s| {
                let seed = s.iter().map(|s| s.as_slice()).collect::<Vec<&[u8]>>();
                Pubkey::create_program_address(&seed, &self.program_id)
                    .expect("Create signer from seeds")
            })
            .collect::<Vec<_>>();

        self.set_account(
            program_cache,
            &SEEDS_PUBKEY,
            &AccountSharedData::new_data(
                Rent::default().minimum_balance(MAX_PERMITTED_DATA_LENGTH as usize),
                &seeds,
                &self.program_id,
            )?,
        )
        .await?;

        let mut accounts_meta = vec![
            AccountMeta::new_readonly(SEEDS_PUBKEY, false),
            AccountMeta::new_readonly(instruction.program_id, false),
        ];
        accounts_meta.extend(instruction.accounts.iter().map(|m| AccountMeta {
            pubkey: m.pubkey,
            is_signer: !signers.contains(&m.pubkey) && m.is_signer,
            is_writable: m.is_writable,
        }));

        let emulate_trx = self
            .prepare_transaction(&[Instruction::new_with_bytes(
                self.program_id,
                &instruction.data,
                accounts_meta,
            )])
            .await?;

        let trx_metadata = match self
            .emulator_context
            .banks_client
            .process_transaction_with_metadata(emulate_trx)
            .await
        {
            Ok(BanksTransactionResultWithMetadata {
                result: Ok(()),
                metadata,
            }) => Ok(metadata),
            Ok(BanksTransactionResultWithMetadata {
                result: Err(err), ..
            })
            | Err(BanksClientError::SimulationError { err, .. })
            | Err(BanksClientError::TransactionError(err)) => match err {
                TransactionError::InstructionError(_, err) => {
                    Err(evm_loader::error::Error::ExternalCallFailed(
                        instruction.program_id,
                        err.to_string(),
                    ))
                }
                _ => Err(evm_loader::error::Error::Custom(err.to_string())),
            },
            Err(err) => Err(evm_loader::error::Error::Custom(err.to_string())),
        }?;

        let next_slot = self
            .emulator_context
            .banks_client
            .get_root_slot()
            .await
            .unwrap()
            + 1;
        self.emulator_context
            .warp_to_slot(next_slot)
            .expect("Warp to next slot");

        // Update writable accounts
        let payer = self.payer();
        for pubkey in instruction.accounts.iter().filter_map(|m| {
            if m.is_writable && m.pubkey != payer {
                Some(m.pubkey)
            } else {
                None
            }
        }) {
            let account = self
                .emulator_context
                .banks_client
                .get_account(pubkey)
                .await
                .unwrap()
                .unwrap_or_default();

            let original = accounts
                .iter()
                .find(|a| a.key == &pubkey)
                .expect("Missing pubkey in accounts map");

            **original.try_borrow_mut_lamports()? = account.lamports;
            if original.data_len() != account.data.len() {
                original.realloc(account.data.len(), true)?;
            }
            original
                .try_borrow_mut_data()?
                .copy_from_slice(account.data.as_slice());
            if *original.owner != account.owner {
                original.assign(&account.owner);
            }
        }

        Ok(trx_metadata)
    }
}

fn from_account_info(account: &AccountInfo) -> AccountSharedData {
    let mut acc = AccountSharedData::new(account.lamports(), 0, account.owner);
    acc.set_data(account.data.as_ref().borrow().to_vec());
    acc.set_executable(account.executable);
    acc.set_rent_epoch(account.rent_epoch);
    acc
}
