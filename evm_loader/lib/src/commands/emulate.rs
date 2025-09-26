use std::collections::HashMap;

// use crate::account_data::AccountData;
use crate::commands::get_config::BuildConfigSimulator;
use crate::config::DbConfig;
use crate::rpc::Rpc;
use crate::rpc::{CallDbClient, RpcEnum};
use crate::sysvar::get_sysvar;
use crate::tracing::tracers::Tracer;
use crate::tracing::{AccountOverride, BlockOverrides};
use crate::types::FromAddress;
use crate::types::TracerDb;
use crate::types::{AccountInfoLevel, EmulateRequest};

use crate::{
    account_storage::{EmulatorAccountStorage, SyncedAccountStorage},
    errors::NeonError,
    types::SerializedAccount,
    NeonResult,
};
use ethnum::U256;
use evm_loader::account_storage::AccountStorage;
use evm_loader::error::build_revert_message;
use evm_loader::types::{Address, Transaction};
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
    pub accounts_data: Option<Vec<(Pubkey, SerializedAccount)>>,
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
        backend: &SyncedExecutorState<EmulatorAccountStorage<impl Rpc>>,
    ) -> Self {
        let revert_message = build_revert_message(&e.to_string());
        let exit_status = ExitStatus::Revert(revert_message);
        Self {
            exit_status: exit_status.to_string(),
            external_solana_call: false,
            reverts_before_solana_calls: false,
            reverts_after_solana_calls: false,
            is_timestamp_number_used: backend.backend().is_timestamp_number_used(),
            result: exit_status.into_result().unwrap_or_default(),
            steps_executed: 0,
            used_gas: 0,
            iterations: 0,
            solana_accounts: vec![],
            logs: backend.backend().logs(),
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

async fn initialize_storage<'rpc, T: Rpc + BuildConfigSimulator>(
    rpc: &'rpc T,
    program_id: &Pubkey,
    emulate_request: &EmulateRequest,
    overrides: Overrides,
) -> NeonResult<EmulatorAccountStorage<'rpc, T>> {
    let storage = EmulatorAccountStorage::with_accounts(
        rpc,
        *program_id,
        &emulate_request.accounts,
        emulate_request.chains.clone(),
        overrides.blocks,
        overrides.states,
        overrides.solana_accounts,
        emulate_request.tx.chain_id,
    )
    .await?;

    // Store the from pubkey in the storage to correctly initialize the BalanceAccount.
    let from = &emulate_request.tx.from;
    if let FromAddress::Solana(pubkey) = from {
        storage.add_balance_pubkey(from.address(), *pubkey);
        info!("from is solana address: {:?}", pubkey);
    }

    Ok(storage)
}

async fn initialize_storage_and_transaction<'rpc, T: Rpc + BuildConfigSimulator>(
    program_id: &Pubkey,
    emulate_request: &EmulateRequest,
    rpc: &'rpc T,
    overrides: Overrides,
) -> NeonResult<(EmulatorAccountStorage<'rpc, T>, Transaction)> {
    let storage = initialize_storage(rpc, program_id, emulate_request, overrides).await?;

    let (origin, tx) = emulate_request.tx.clone().into_transaction(&storage).await;

    info!("origin: {:?}", origin);
    info!("tx: {:?}", tx);

    let chain_id = tx.chain_id().unwrap_or_else(|| storage.default_chain_id());

    storage
        .mark_balance_account(&origin, chain_id, true)
        .await?;

    Ok((storage, tx))
}

async fn increment_nonce<T: Rpc + BuildConfigSimulator>(
    storage: &mut EmulatorAccountStorage<'_, T>,
    origin: &Address,
    chain_id: u64,
) -> NeonResult<()> {
    storage.increment_nonce(*origin, chain_id).await?;

    Ok(())
}

async fn transfer_gas_limit<T: Rpc + BuildConfigSimulator>(
    storage: &mut EmulatorAccountStorage<'_, T>,
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

async fn calculate_response<T: Rpc + BuildConfigSimulator, Tr: Tracer>(
    steps_executed: u64,
    step_on_solana: Option<u64>,
    exit_status: ExitStatus,
    storage: &EmulatorAccountStorage<'_, T>,
    tracer: Option<Tr>,
    provide_account_info: Option<AccountInfoLevel>,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    debug!("Execute done, result={exit_status:?}");
    debug!("{steps_executed} steps executed");
    if let Some(step) = step_on_solana {
        debug!("Got Solana call on {step}");
    }

    let logs = storage.logs();
    let execute_status = storage.execute_status;

    let steps_iterations = 1.max(steps_executed.div_ceil(EVM_STEPS_MIN));

    let begin_end_iterations = 2;
    let iterations: u64 = steps_iterations + begin_end_iterations + storage.realloc_iterations;
    let iterations_gas = iterations * LAMPORTS_PER_SIGNATURE;
    let treasury_gas = iterations * PAYMENT_TO_TREASURE;
    let storage_gas = storage.get_changes_in_rent()?;

    let used_gas = storage_gas + iterations_gas + treasury_gas;

    let solana_accounts = storage
        .used_accounts()
        .iter()
        .map(|v| SolanaAccount {
            pubkey: v.pubkey,
            is_writable: v.is_writable,
        })
        .collect::<Vec<_>>();

    let mut result = (
        EmulateResponse {
            exit_status: exit_status.to_string(),
            external_solana_call: execute_status.external_solana_call,
            reverts_before_solana_calls: execute_status.reverts_before_solana_calls,
            reverts_after_solana_calls: execute_status.reverts_after_solana_calls,
            is_timestamp_number_used: storage.is_timestamp_number_used(),
            steps_executed,
            used_gas,
            solana_accounts,
            result: exit_status.into_result().unwrap_or_default(),
            iterations,
            logs,
            accounts_data: None,
        },
        tracer.map(|tracer| tracer.into_traces(used_gas)),
    );

    if let Some(level) = provide_account_info {
        result.0.accounts_data =
            Some(provide_account_data(storage, &result.0.solana_accounts, level).await?);
    };

    Ok(result)
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
    storage: &mut EmulatorAccountStorage<'_, impl BuildConfigSimulator>,
    tx: &Transaction,
    tracer: Option<T>,
    emulate_request: &EmulateRequest,
    step_limit: u64,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let origin = emulate_request.tx.from.address();

    let (exit_status, steps_executed, step_on_solana, tracer, timestamped_contracts) = {
        let mut backend = SyncedExecutorState::new(storage);
        let mut evm = match Machine::new(tx, origin, &mut backend, tracer).await {
            Ok(evm) => evm,
            Err(e) => {
                error!("EVM creation failed {e:?}");
                return Ok((EmulateResponse::revert(&e, &backend), None));
            }
        };

        let (exit_status, steps_executed, step_on_solana, tracer) =
            evm.execute(step_limit, &mut backend).await?;

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
            step_on_solana,
            tracer,
            timestamped_contracts,
        )
    };

    storage.mark_timestamped_contracts(timestamped_contracts.keys());

    calculate_response(
        steps_executed,
        step_on_solana,
        exit_status,
        storage,
        tracer,
        emulate_request.provide_account_info,
    )
    .await
}

async fn prepare_origin<T: Rpc + BuildConfigSimulator>(
    origin: &Address,
    storage: &mut EmulatorAccountStorage<'_, T>,
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

    overrides.blocks.get_or_insert(BlockOverrides {
        number: Some(clock.slot),
        time: Some(clock.unix_timestamp),
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

    let (exit_status, steps_executed, step_on_solana, tracer, timestamped_contracts) = {
        let mut backend = SyncedExecutorState::new(&mut storage);

        let mut evm = match Machine::new(&tx, origin, &mut backend, tracer).await {
            Ok(evm) => evm,
            Err(e) => {
                error!("EVM creation failed {e:?}");
                return Ok((EmulateResponse::revert(&e, &backend), None));
            }
        };

        let mut exit_status = ExitStatus::StepLimit;
        let mut steps_executed = 0u64;
        let mut step_on_solana = None;
        let mut tracer_result: Option<T> = evm.take_tracer();
        for execution_step in &execution_map.steps {
            if execution_step.is_reset || execution_step.is_cancel {
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

                backend = SyncedExecutorState::new(&mut storage);
                evm = match Machine::new(&tx, origin, &mut backend, tracer_result).await {
                    Ok(evm) => evm,
                    Err(e) => {
                        error!("EVM creation failed {e:?}");
                        return Ok((EmulateResponse::revert(&e, &backend), None));
                    }
                };
                tracer_result = evm.take_tracer();
            }

            if execution_step.is_cancel {
                evm.set_tracer(tracer_result);

                evm.end_vm(&backend, ExitStatus::Cancel).await?;
                exit_status = ExitStatus::Cancel;

                tracer_result = evm.take_tracer();
                break;
            }

            match exit_status {
                ExitStatus::Return(_) | ExitStatus::Stop => {
                    if execution_step.steps == 0 {
                        // skipping empty instructions
                        continue;
                    }
                }
                _ => (),
            }

            evm.set_tracer(tracer_result);
            let (local_exit_status, local_steps_executed, local_step_on_solana, local_tracer) = evm
                .execute(u64::from(execution_step.steps), &mut backend)
                .await?;

            exit_status = local_exit_status;
            steps_executed += local_steps_executed;
            step_on_solana = local_step_on_solana;
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
            step_on_solana,
            tracer_result,
            timestamped_contracts,
        )
    };

    storage.mark_timestamped_contracts(timestamped_contracts.keys());

    calculate_response(
        steps_executed,
        step_on_solana,
        exit_status,
        &storage,
        tracer,
        emulate_request.provide_account_info,
    )
    .await
}

async fn provide_account_data(
    storage: &EmulatorAccountStorage<'_, impl Rpc>,
    solana_accounts: &[SolanaAccount],
    level: AccountInfoLevel,
) -> NeonResult<Vec<(Pubkey, SerializedAccount)>> {
    let pubkeys = solana_accounts
        .iter()
        .filter_map(|v| {
            if v.is_writable || AccountInfoLevel::Changed != level {
                Some(v.pubkey)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let result = storage.get_multiple_accounts(&pubkeys).await?;

    Ok(pubkeys
        .iter()
        .zip(result.into_iter())
        .filter_map(|(pubkey, account)| {
            account.map(|acc| /*AccountData::new_from_account(*pubkey, &acc)*/
                (*pubkey, SerializedAccount::from(acc))
            )
        })
        .collect::<Vec<_>>()
    )
}
