use base64::Engine;
use std::collections::BTreeMap;
use tokio::sync::{Mutex, MutexGuard, OnceCell};

use serde::{Deserialize, Serialize};
use solana_program_test::{ProgramTest, ProgramTestContext};
use solana_sdk::{
    account::{Account, AccountSharedData},
    account_utils::StateMut,
    bpf_loader, bpf_loader_deprecated,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    instruction::Instruction,
    pubkey::Pubkey,
    rent::Rent,
    signer::Signer,
    transaction::Transaction,
};

use crate::{rpc::Rpc, NeonError, NeonResult};

use serde_with::{serde_as, DisplayFromStr};

#[derive(Debug, Serialize)]
pub enum Status {
    Ok,
    Emergency,
    Unknown,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainInfo {
    pub id: u64,
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub token: Pubkey,
}

#[serde_as]
#[derive(Debug, Serialize)]
pub struct GetConfigResponse {
    pub version: String,
    pub revision: String,
    pub status: Status,
    pub environment: String,
    pub chains: Vec<ChainInfo>,
    pub config: BTreeMap<String, String>,
}

static PROGRAM_TEST: OnceCell<Mutex<ProgramTestContext>> = OnceCell::const_new();

async fn read_program_data_from_account(
    rpc_client: &dyn Rpc,
    program_id: Pubkey,
) -> NeonResult<Vec<u8>> {
    let Some(account) = rpc_client.get_account(&program_id).await?.value else {
        return Err(NeonError::AccountNotFound(program_id));
    };

    if account.owner == bpf_loader::id() || account.owner == bpf_loader_deprecated::id() {
        return Ok(account.data);
    }

    if account.owner != bpf_loader_upgradeable::id() {
        return Err(NeonError::AccountIsNotBpf(program_id));
    }

    if let Ok(UpgradeableLoaderState::Program {
        programdata_address,
    }) = account.state()
    {
        let Some(programdata_account) = rpc_client.get_account(&programdata_address).await?.value else {
            return Err(NeonError::AssociatedPdaNotFound(programdata_address, program_id));
        };

        let offset = UpgradeableLoaderState::size_of_programdata_metadata();
        let program_data = &programdata_account.data[offset..];

        Ok(program_data.to_vec())
    } else {
        Err(NeonError::AccountIsNotUpgradeable(program_id))
    }
}

async fn lock_program_test(
    program_id: Pubkey,
    program_data: Vec<u8>,
) -> MutexGuard<'static, ProgramTestContext> {
    async fn init_program_test() -> Mutex<ProgramTestContext> {
        let program_test = ProgramTest::default();
        let context = program_test.start_with_context().await;
        Mutex::new(context)
    }

    let mut context = PROGRAM_TEST
        .get_or_init(init_program_test)
        .await
        .lock()
        .await;

    context.set_account(
        &program_id,
        &AccountSharedData::from(Account {
            lamports: Rent::default().minimum_balance(program_data.len()).max(1),
            data: program_data,
            owner: bpf_loader::id(),
            executable: true,
            rent_epoch: 0,
        }),
    );

    context
}

enum ConfigSimulator<'r> {
    Rpc(Pubkey, &'r dyn Rpc),
    ProgramTest(MutexGuard<'static, ProgramTestContext>),
}

impl<'r> ConfigSimulator<'r> {
    pub async fn new(
        rpc_client: &'r dyn Rpc,
        program_id: Pubkey,
    ) -> NeonResult<ConfigSimulator<'r>> {
        let simulator = if rpc_client.can_simulate_transaction() {
            let identity = rpc_client.get_account_with_sol().await?;
            Self::Rpc(identity, rpc_client)
        } else {
            let program_data = read_program_data_from_account(rpc_client, program_id).await?;
            let mut program_test = lock_program_test(program_id, program_data).await;
            program_test.get_new_latest_blockhash().await?;

            Self::ProgramTest(program_test)
        };

        Ok(simulator)
    }

    async fn simulate_config(
        &mut self,
        program_id: Pubkey,
        instruction: u8,
        data: &[u8],
    ) -> NeonResult<Vec<u8>> {
        fn base64_decode(s: &str) -> Vec<u8> {
            base64::engine::general_purpose::STANDARD.decode(s).unwrap()
        }

        let input = [&[instruction], data].concat();

        let logs = match self {
            ConfigSimulator::Rpc(signer, rpc) => {
                let result = rpc
                    .simulate_transaction(
                        Some(*signer),
                        &[Instruction::new_with_bytes(program_id, &input, vec![])],
                    )
                    .await?
                    .value;

                if let Some(e) = result.err {
                    return Err(e.into());
                }
                result.logs.unwrap()
            }
            ConfigSimulator::ProgramTest(context) => {
                let payer_pubkey = context.payer.pubkey();
                let tx = Transaction::new_signed_with_payer(
                    &[Instruction::new_with_bytes(program_id, &input, vec![])],
                    Some(&payer_pubkey),
                    &[&context.payer],
                    context.last_blockhash,
                );
                let result = context
                    .banks_client
                    .simulate_transaction(tx)
                    .await
                    .map_err(|e| NeonError::from(Box::new(e)))?;

                result.result.unwrap()?;
                result.simulation_details.unwrap().logs
            }
        };

        // Program return: 53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io AQAAAAAAAAA=
        let return_data = logs
            .into_iter()
            .find_map(|msg| {
                let prefix = std::format!("Program return: {program_id} ");
                msg.strip_prefix(&prefix).map(base64_decode)
            })
            .unwrap();

        Ok(return_data)
    }
}

async fn get_version(
    context: &mut ConfigSimulator<'_>,
    program_id: Pubkey,
) -> NeonResult<(String, String)> {
    let return_data = context.simulate_config(program_id, 0xA7, &[]).await?;
    let (version, revision) = bincode::deserialize(&return_data)?;

    Ok((version, revision))
}

async fn get_status(context: &mut ConfigSimulator<'_>, program_id: Pubkey) -> NeonResult<Status> {
    let return_data = context.simulate_config(program_id, 0xA6, &[]).await?;
    match return_data[0] {
        0 => Ok(Status::Emergency),
        1 => Ok(Status::Ok),
        _ => Ok(Status::Unknown),
    }
}

async fn get_environment(
    context: &mut ConfigSimulator<'_>,
    program_id: Pubkey,
) -> NeonResult<String> {
    let return_data = context.simulate_config(program_id, 0xA2, &[]).await?;
    let environment = String::from_utf8(return_data)?;

    Ok(environment)
}

async fn get_chains(
    context: &mut ConfigSimulator<'_>,
    program_id: Pubkey,
) -> NeonResult<Vec<ChainInfo>> {
    let mut result = Vec::new();

    let return_data = context.simulate_config(program_id, 0xA0, &[]).await?;
    let chain_count = return_data.as_slice().try_into()?;
    let chain_count = usize::from_le_bytes(chain_count);

    for i in 0..chain_count {
        let index = i.to_le_bytes();
        let return_data = context.simulate_config(program_id, 0xA1, &index).await?;

        let (id, name, token) = bincode::deserialize(&return_data)?;
        result.push(ChainInfo { id, name, token });
    }

    Ok(result)
}

async fn get_properties(
    context: &mut ConfigSimulator<'_>,
    program_id: Pubkey,
) -> NeonResult<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();

    let return_data = context.simulate_config(program_id, 0xA3, &[]).await?;
    let count = return_data.as_slice().try_into()?;
    let count = usize::from_le_bytes(count);

    for i in 0..count {
        let index = i.to_le_bytes();
        let return_data = context.simulate_config(program_id, 0xA4, &index).await?;

        let (name, value) = bincode::deserialize(&return_data)?;
        result.insert(name, value);
    }

    Ok(result)
}

pub async fn execute(rpc_client: &dyn Rpc, program_id: Pubkey) -> NeonResult<GetConfigResponse> {
    let mut simulator = ConfigSimulator::new(rpc_client, program_id).await?;

    let (version, revision) = get_version(&mut simulator, program_id).await?;

    Ok(GetConfigResponse {
        version,
        revision,
        status: get_status(&mut simulator, program_id).await?,
        environment: get_environment(&mut simulator, program_id).await?,
        chains: get_chains(&mut simulator, program_id).await?,
        config: get_properties(&mut simulator, program_id).await?,
    })
}

pub async fn read_chains(rpc_client: &dyn Rpc, program_id: Pubkey) -> NeonResult<Vec<ChainInfo>> {
    let mut simulator = ConfigSimulator::new(rpc_client, program_id).await?;

    let chains = get_chains(&mut simulator, program_id).await?;
    Ok(chains)
}