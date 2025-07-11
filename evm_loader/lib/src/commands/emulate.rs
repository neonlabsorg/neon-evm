use std::collections::HashMap;

use crate::account_data::AccountData;
use crate::commands::get_config::BuildConfigSimulator;
use crate::config::DbConfig;
use crate::emulator_platform::EmulatorPlatform;
use crate::rpc::Rpc;
use crate::rpc::{CallDbClient, RpcEnum};
use crate::sysvar::get_sysvar;
use crate::tracing::tracers::{Tracer, TracerTypeEnum};
use crate::tracing::{AccountOverride, BlockOverrides};
use crate::types::{AccountInfoLevel, EmulateFromHolderApiRequest, EmulateRequest};
use crate::types::{FromAddress, TracerDb};

use crate::{errors::NeonError, NeonResult};
use ethnum::U256;
use evm_loader::account_storage::{AccountStorage, SyncedAccountStorage};
use evm_loader::error::build_revert_message;
use evm_loader::platform::Platform;
use evm_loader::types::{Address, Transaction, TrxView};
use evm_loader::{
    config::{
        EVM_STEPS_MIN, GAS_LIMIT_MULTIPLIER_NO_CHAINID, LAMPORTS_PER_SIGNATURE, PAYMENT_TO_TREASURE,
    },
    evm::{ExitStatus, Machine},
    executor::SyncedExecutorState,
};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{hex::Hex, serde_as, DisplayFromStr};
use solana_sdk::clock::Clock;
use solana_sdk::{account::Account, pubkey::Pubkey};
use web3::types::Log;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaAccount {
    #[serde_as(as = "DisplayFromStr")]
    pub pubkey: Pubkey,
    pub is_writable: bool,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct EmulateResponse {
    pub exit_status: String,
    pub external_solana_call: bool,
    pub reverts_before_solana_calls: bool,
    pub reverts_after_solana_calls: bool,
    pub is_timestamp_number_used: bool,
    #[serde_as(as = "Hex")]
    pub result: Vec<u8>,
    pub steps_executed: u64,
    pub used_gas: u64,
    pub iterations: u64,
    pub solana_accounts: Vec<SolanaAccount>,
    pub logs: Vec<Log>,
    pub accounts_data: Option<Vec<AccountData>>,
}

#[derive(Clone)]
struct Overrides {
    pub blocks: Option<BlockOverrides>,
    pub states: Option<HashMap<Address, AccountOverride>>,
    pub solana_accounts: Option<HashMap<Pubkey, Option<Account>>>,
}

impl EmulateResponse {
    pub fn revert<E: ToString>(
        e: &E,
        backend: &SyncedExecutorState<EmulatorPlatform<impl Rpc>>,
    ) -> Self {
        let revert_message = build_revert_message(&e.to_string());
        let exit_status = ExitStatus::Revert(revert_message);
        Self {
            exit_status: exit_status.to_string(),
            external_solana_call: false,
            reverts_before_solana_calls: false,
            reverts_after_solana_calls: false,
            is_timestamp_number_used: backend.backend().is_clock_used(),
            result: exit_status.into_result().unwrap_or_default(),
            steps_executed: 0,
            used_gas: 0,
            iterations: 0,
            solana_accounts: vec![],
            logs: backend.backend().logs().to_vec(),
            accounts_data: None,
        }
    }
}

fn init_overrides(emulate_request: &EmulateRequest) -> Overrides {
    let blocks = emulate_request
        .trace_config
        .as_ref()
        .and_then(|t| t.block_overrides.clone());
    let states = emulate_request
        .trace_config
        .as_ref()
        .and_then(|t| t.state_overrides.clone());

    let solana_accounts = emulate_request.solana_overrides.clone().map(|overrides| {
        overrides
            .iter()
            .map(|(pubkey, account)| (*pubkey, account.as_ref().map(Account::from)))
            .collect()
    });

    Overrides {
        blocks,
        states,
        solana_accounts,
    }
}

pub async fn execute_from_holder(
    rpc: &impl BuildConfigSimulator,
    program_id: &Pubkey,
    emulate_request: EmulateFromHolderApiRequest,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let holder_key = emulate_request.holder_pubkey;

    let response = crate::commands::get_holder::execute(rpc, program_id, holder_key).await?;

    match response.status {
        crate::commands::get_holder::Status::Empty => Err(NeonError::AccountNotFound(holder_key)),
        crate::commands::get_holder::Status::Active => {
            execute(
                rpc,
                None,
                program_id,
                EmulateRequest {
                    tx: response
                        .tx_data
                        .ok_or(NeonError::AccountInvalidStatus(holder_key))?,
                    step_limit: emulate_request.step_limit,
                    chains: emulate_request.chains,
                    trace_config: None,
                    accounts: response.accounts.unwrap_or(Vec::new()),
                    solana_overrides: None,
                    provide_account_info: None,
                    execution_map: None,
                },
                None::<TracerTypeEnum>,
            )
            .await
        }
        _ => Err(NeonError::AccountInvalidStatus(holder_key)),
    }
}

pub async fn execute<T: Tracer>(
    rpc: &impl BuildConfigSimulator,
    db_config: Option<&DbConfig>,
    program_id: &Pubkey,
    emulate_request: EmulateRequest,
    tracer: Option<T>,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let step_limit = emulate_request.step_limit.unwrap_or(100_000);

    let result = emulate_trx(
        &emulate_request,
        db_config,
        program_id,
        step_limit,
        tracer,
        rpc,
    )
    .await?;

    Ok(result)
}

async fn create_rpc(
    db_config: Option<&DbConfig>,
    block: u64,
    index: Option<u64>,
) -> NeonResult<RpcEnum> {
    Ok(RpcEnum::CallDbClient(
        CallDbClient::new(
            TracerDb::maybe_from_config(db_config)
                .await
                .clone()
                .expect("TracerDB must be configured for CallDbClient"),
            block,
            index,
        )
        .await?,
    ))
}

async fn create_platform<'rpc, T: Rpc + BuildConfigSimulator>(
    rpc: &'rpc T,
    program_id: Pubkey,
    emulate_request: &EmulateRequest,
    overrides: Overrides,
) -> NeonResult<EmulatorPlatform<&'rpc T>> {
    let chains = match &emulate_request.chains {
        Some(chains) => chains.clone(),
        None => super::get_config::read_chains(rpc, program_id).await?,
    };

    let accounts_hint: &[Pubkey] = &emulate_request.accounts;
    let mut platform = EmulatorPlatform::new(rpc, program_id, &chains, accounts_hint).await?;

    let chain_id = emulate_request
        .tx
        .chain_id
        .unwrap_or_else(|| platform.default_chain());

    // Overrides
    if let Some(block_overrides) = overrides.blocks {
        platform.override_clock(block_overrides).await?;
    }
    if let Some(solana_accounts) = overrides.solana_accounts {
        platform.override_solana_accounts(solana_accounts);
    }
    if let Some(states) = overrides.states {
        platform.override_accounts(chain_id, states).await?;
    }

    // Initialize BalanceAccount of Solana user.
    if let FromAddress::Solana(pubkey) = emulate_request.tx.from {
        platform.create_balance_for_solana_user(pubkey).await?;
    }

    Ok(platform)
}

async fn initialize_storage_and_transaction<'rpc, T: Rpc + BuildConfigSimulator>(
    program_id: &Pubkey,
    emulate_request: &EmulateRequest,
    rpc: &'rpc T,
    overrides: Overrides,
) -> NeonResult<(EmulatorPlatform<&'rpc T>, Transaction)> {
    let mut storage = create_platform(rpc, *program_id, emulate_request, overrides).await?;

    let (origin, tx) = emulate_request.tx.clone().into_transaction(&storage).await;

    info!("origin: {:?}", origin);
    info!("tx: {:?}", tx);

    let chain_id = tx.chain_id().unwrap_or_else(|| storage.default_chain_id());
    storage.create_balance(origin, chain_id).await?;

    Ok((storage, tx))
}

async fn increment_nonce<T: Rpc>(
    storage: &mut EmulatorPlatform<T>,
    origin: &Address,
    chain_id: u64,
) -> NeonResult<()> {
    storage.increment_nonce(*origin, chain_id).await?;

    Ok(())
}

async fn transfer_gas_limit<T: Rpc>(
    storage: &mut EmulatorPlatform<T>,
    tx: &Transaction,
    origin: &Address,
    chain_id: u64,
    increase_gas_limit: bool,
) -> NeonResult<()> {
    let mut gas_limit = tx.gas_limit_in_tokens()?;

    if increase_gas_limit {
        gas_limit = gas_limit.saturating_mul(U256::from(GAS_LIMIT_MULTIPLIER_NO_CHAINID));
    }
    storage.burn(*origin, chain_id, gas_limit).await?;

    Ok(())
}

async fn calculate_response(
    steps_executed: u64,
    exit_status: ExitStatus,
    platform: &EmulatorPlatform<impl Rpc>,
    tracer: Option<impl Tracer>,
    provide_account_info: Option<AccountInfoLevel>,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    debug!("Execute done, result={exit_status:?}");
    debug!("{steps_executed} steps executed");

    let execute_status = platform.execute_status();

    let steps_iterations = 1.max(steps_executed.div_ceil(EVM_STEPS_MIN));

    let begin_end_iterations = 2;
    let iterations: u64 = steps_iterations + begin_end_iterations + platform.realloc_iterations();
    let iterations_gas = iterations * LAMPORTS_PER_SIGNATURE;
    let treasury_gas = iterations * PAYMENT_TO_TREASURE;
    let storage_gas = platform.required_lamports().await?;

    let used_gas = storage_gas + iterations_gas + treasury_gas;

    let solana_accounts = platform.used_solana_accounts();
    let accounts_data = if let Some(level) = provide_account_info {
        Some(platform.provide_account_data(level)?)
    } else {
        None
    };

    let response = EmulateResponse {
        exit_status: exit_status.to_string(),
        external_solana_call: execute_status.external_solana_call,
        reverts_before_solana_calls: execute_status.reverts_before_solana_calls,
        reverts_after_solana_calls: execute_status.reverts_after_solana_calls,
        is_timestamp_number_used: platform.is_clock_used(),
        steps_executed,
        used_gas,
        solana_accounts,
        result: exit_status.into_result().unwrap_or_default(),
        iterations,
        logs: platform.logs().to_vec(),
        accounts_data,
    };

    let tracer_result = tracer.map(|tracer| tracer.into_traces(used_gas));

    Ok((response, tracer_result))
}

pub async fn mark_timestamped_contracts<'r>(
    storage: &EmulatorPlatform<impl Rpc>,
    contracts: impl Iterator<Item = &'r Address>,
) -> NeonResult<()> {
    let clock = get_sysvar::<Clock>(storage).await?;

    for address in contracts.copied() {
        let Some(mut contract) = storage.get_contract(address).await? else {
            continue;
        };
        contract.update_timestamp_used_at(&clock)?;
    }

    Ok(())
}

async fn emulate_trx<T: Tracer>(
    emulate_request: &EmulateRequest,
    db_config: Option<&DbConfig>,
    program_id: &Pubkey,
    step_limit: u64,
    tracer: Option<T>,
    rpc: &impl BuildConfigSimulator,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    info!("tx_params: {:?}", emulate_request.tx);

    if emulate_request.execution_map.is_none() {
        let overrides = init_overrides(emulate_request);
        let (mut storage, tx) =
            initialize_storage_and_transaction(program_id, emulate_request, rpc, overrides).await?;

        let chain_id = emulate_request
            .tx
            .chain_id
            .unwrap_or_else(|| storage.default_chain_id());
        let from = emulate_request.tx.from.address();

        increment_nonce(&mut storage, &from, chain_id).await?;

        let result =
            emulate_trx_single_step(&mut storage, &tx, tracer, emulate_request, step_limit).await?;

        return Ok(result);
    }

    emulate_trx_multiple_steps(db_config, program_id, tracer, emulate_request, step_limit).await
}

async fn emulate_trx_single_step<T: Tracer>(
    storage: &mut EmulatorPlatform<impl Rpc>,
    tx: &Transaction,
    tracer: Option<T>,
    emulate_request: &EmulateRequest,
    step_limit: u64,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let origin = emulate_request.tx.from.address();

    let (exit_status, steps_executed, tracer, timestamped_contracts) = {
        let mut backend = SyncedExecutorState::new(storage).await;
        let mut evm = match Machine::with_tracer(tx, origin, &mut backend, tracer).await {
            Ok(evm) => evm,
            Err(e) => {
                error!("EVM creation failed {e:?}");
                return Ok((EmulateResponse::revert(&e, &backend), None));
            }
        };

        let (exit_status, steps_executed) = evm.execute(step_limit, &mut backend).await?;
        let tracer = evm.into_tracer();

        if exit_status == ExitStatus::StepLimit {
            error!("Step_limit={step_limit} exceeded");
            return Ok((
                EmulateResponse::revert(&NeonError::TooManySteps, &backend),
                None,
            ));
        }

        let timestamped_contracts = backend.timestamped_contracts.take();
        (exit_status, steps_executed, tracer, timestamped_contracts)
    };

    mark_timestamped_contracts(storage, timestamped_contracts.keys()).await?;

    calculate_response(
        steps_executed,
        exit_status,
        storage,
        tracer,
        emulate_request.provide_account_info,
    )
    .await
}

async fn prepare_origin<T: Rpc>(
    origin: &Address,
    storage: &mut EmulatorPlatform<T>,
    tx: &Transaction,
    chain_id: u64,
    increase_gas_limit: bool,
    is_skd_transaction: bool,
) -> NeonResult<()> {
    if is_skd_transaction {
        // Increment origin's nonce only once for the whole execution tree.
        let tx_nonce = tx.nonce();
        let origin_nonce = storage.nonce(*origin, chain_id).await;

        if origin_nonce == tx_nonce {
            increment_nonce(storage, origin, chain_id).await?;
        }
    } else {
        increment_nonce(storage, origin, chain_id).await?;
        transfer_gas_limit(storage, tx, origin, chain_id, increase_gas_limit).await?;
    }

    Ok(())
}

async fn emulate_trx_multiple_steps<T: Tracer>(
    db_config: Option<&DbConfig>,
    program_id: &Pubkey,
    tracer: Option<T>,
    emulate_request: &EmulateRequest,
    step_limit: u64,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let execution_map = emulate_request
        .execution_map
        .clone()
        .expect("execution map must be not empty");

    let is_skd_transaction = execution_map.is_skd_transaction;

    let origin = emulate_request.tx.from.address();
    let (block, index) = {
        let step = execution_map
            .steps
            .first()
            .expect("execution map must be not empty")
            .clone();
        (step.block, step.index)
    };

    let mut rpc = create_rpc(db_config, block, index).await?;

    let clock = get_sysvar::<Clock>(&rpc).await?;

    let mut overrides = init_overrides(emulate_request);

    let block_number = execution_map.block_number.or(Some(clock.slot));
    let block_timestamp = execution_map.block_timestamp.or(Some(clock.unix_timestamp));

    overrides.blocks.get_or_insert(BlockOverrides {
        number: block_number,
        time: block_timestamp,
        ..Default::default()
    });

    let (mut storage, mut tx) =
        initialize_storage_and_transaction(program_id, emulate_request, &rpc, overrides.clone())
            .await?;

    let chain_id = emulate_request
        .tx
        .chain_id
        .unwrap_or_else(|| storage.default_chain_id());
    let increase_gas_limit = emulate_request.tx.chain_id.is_none();

    prepare_origin(
        &origin,
        &mut storage,
        &tx,
        chain_id,
        increase_gas_limit,
        is_skd_transaction,
    )
    .await?;

    let (exit_status, steps_executed, tracer, timestamped_contracts) = {
        let mut backend = SyncedExecutorState::new(&mut storage).await;

        let mut evm = match Machine::with_tracer(&tx, origin, &mut backend, tracer).await {
            Ok(evm) => evm,
            Err(e) => {
                error!("EVM creation failed {e:?}");
                return Ok((EmulateResponse::revert(&e, &backend), None));
            }
        };

        let mut exit_status = ExitStatus::StepLimit;
        let mut steps_executed = 0u64;
        let mut tracer_result: Option<T> = evm.take_tracer();
        for execution_step in &execution_map.steps {
            if execution_step.is_reset {
                drop(evm);
                drop(backend);
                drop(storage);
                drop(rpc);

                steps_executed = 0u64;
                exit_status = ExitStatus::StepLimit;

                rpc = create_rpc(db_config, execution_step.block, execution_step.index).await?;
                (storage, tx) = initialize_storage_and_transaction(
                    program_id,
                    emulate_request,
                    &rpc,
                    overrides.clone(),
                )
                .await?;

                if let Some(ref mut tracer) = tracer_result {
                    tracer.clear(&emulate_request.tx);
                }

                backend = SyncedExecutorState::new(&mut storage).await;
                evm = match Machine::with_tracer(&tx, origin, &mut backend, tracer_result).await {
                    Ok(evm) => evm,
                    Err(e) => {
                        error!("EVM creation failed {e:?}");
                        return Ok((EmulateResponse::revert(&e, &backend), None));
                    }
                };
                tracer_result = evm.take_tracer();
            }

            if execution_step.is_cancel {
                drop(evm);
                drop(backend);
                drop(storage);
                drop(rpc);

                steps_executed = 0u64;
                exit_status = ExitStatus::Cancel;

                rpc = create_rpc(db_config, execution_step.block, execution_step.index).await?;
                (storage, _) = initialize_storage_and_transaction(
                    program_id,
                    emulate_request,
                    &rpc,
                    overrides.clone(),
                )
                .await?;
                backend = SyncedExecutorState::new(&mut storage).await;

                if let Some(ref mut tracer) = tracer_result {
                    tracer.cancel(&emulate_request.tx);
                }

                break;
            }

            match exit_status {
                ExitStatus::Return(_) | ExitStatus::Stop | ExitStatus::Revert(_) => {
                    if execution_step.steps == 0 {
                        // skipping empty instructions
                        continue;
                    }
                }
                _ => (),
            }

            evm.set_tracer(tracer_result);
            let (local_exit_status, local_steps_executed) = evm
                .execute(u64::from(execution_step.steps), &mut backend)
                .await?;

            let local_tracer = evm.take_tracer();

            exit_status = local_exit_status;
            steps_executed += local_steps_executed;
            tracer_result = local_tracer;
        }

        if exit_status == ExitStatus::StepLimit {
            error!("Step_limit={step_limit} exceeded");
            return Ok((
                EmulateResponse::revert(&NeonError::TooManySteps, &backend),
                None,
            ));
        }

        let timestamped_contracts = backend.timestamped_contracts.take();
        (
            exit_status,
            steps_executed,
            tracer_result,
            timestamped_contracts,
        )
    };

    mark_timestamped_contracts(&storage, timestamped_contracts.keys()).await?;

    calculate_response(
        steps_executed,
        exit_status,
        &storage,
        tracer,
        emulate_request.provide_account_info,
    )
    .await
}
