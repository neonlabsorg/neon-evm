use std::collections::HashMap;

use crate::account_data::AccountData;
use crate::commands::get_config::BuildConfigSimulator;
use crate::config::DbConfig;
use crate::rpc::Rpc;
use crate::rpc::{CallDbClient, RpcEnum};
use crate::tracing::tracers::Tracer;
use crate::tracing::{AccountOverride, BlockOverrides};
use crate::types::TracerDb;
use crate::types::{AccountInfoLevel, EmulateRequest};
use crate::{
    account_storage::{EmulatorAccountStorage, SyncedAccountStorage},
    errors::NeonError,
    NeonResult,
};
use evm_loader::account_storage::AccountStorage;
use evm_loader::error::build_revert_message;
use evm_loader::types::{Address, Transaction};
use evm_loader::{
    config::{EVM_STEPS_MIN, PAYMENT_TO_TREASURE},
    evm::{ExitStatus, Machine},
    executor::SyncedExecutorState,
    gasometer::LAMPORTS_PER_SIGNATURE,
};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{hex::Hex, serde_as, DisplayFromStr};
use solana_sdk::{account::Account, pubkey::Pubkey};
use web3::types::Log;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaAccount {
    #[serde_as(as = "DisplayFromStr")]
    pub pubkey: Pubkey,
    pub is_writable: bool,
    pub is_legacy: bool,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateResponse {
    pub exit_status: String,
    pub external_solana_call: bool,
    pub reverts_before_solana_calls: bool,
    pub reverts_after_solana_calls: bool,
    #[serde_as(as = "Hex")]
    pub result: Vec<u8>,
    pub steps_executed: u64,
    pub used_gas: u64,
    pub iterations: u64,
    pub solana_accounts: Vec<SolanaAccount>,
    pub logs: Vec<Log>,
    pub accounts_data: Option<Vec<AccountData>>,
}

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
    rpc: &(impl Rpc + BuildConfigSimulator),
    db_config: &Option<DbConfig>,
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
    db_config: &Option<DbConfig>,
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
) -> NeonResult<EmulatorAccountStorage<'rpc, T>> {
    let overrides: Overrides = init_overrides(emulate_request);

    EmulatorAccountStorage::with_accounts(
        rpc,
        *program_id,
        &emulate_request.accounts,
        emulate_request.chains.clone(),
        overrides.blocks,
        overrides.states,
        overrides.solana_accounts,
        emulate_request.tx.chain_id,
    )
    .await
}

async fn initialize_storage_from_other<'rpc, T: Rpc + BuildConfigSimulator>(
    storage: &EmulatorAccountStorage<'rpc, T>,
    block_shift: u64,
    timestamp_shift: i64,
    chain_id: Option<u64>,
) -> NeonResult<EmulatorAccountStorage<'rpc, T>> {
    EmulatorAccountStorage::new_from_other(storage, block_shift, timestamp_shift, chain_id).await
}

async fn initialize_storage_and_transaction<'rpc, T: Rpc + BuildConfigSimulator>(
    program_id: &Pubkey,
    emulate_request: &EmulateRequest,
    rpc: &'rpc T,
) -> NeonResult<(EmulatorAccountStorage<'rpc, T>, Transaction)> {
    let mut storage = initialize_storage(rpc, program_id, emulate_request).await?;

    let (origin, tx) = emulate_request.tx.clone().into_transaction(&storage).await;

    info!("origin: {:?}", origin);
    info!("tx: {:?}", tx);

    let chain_id = tx.chain_id().unwrap_or_else(|| storage.default_chain_id());
    storage.increment_nonce(origin, chain_id).await?;

    Ok((storage, tx))
}

async fn calculate_response<T: Rpc + BuildConfigSimulator, Tr: Tracer>(
    steps_executed: u64,
    exit_status: ExitStatus,
    storage: &EmulatorAccountStorage<'_, T>,
    tracer: Option<Tr>,
    provide_account_info: &Option<AccountInfoLevel>,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    debug!("Execute done, result={exit_status:?}");
    debug!("{steps_executed} steps executed");

    let logs = storage.logs();
    let execute_status = storage.execute_status;

    let steps_iterations = (steps_executed + (EVM_STEPS_MIN - 1)) / EVM_STEPS_MIN;
    let treasury_gas = steps_iterations * PAYMENT_TO_TREASURE;
    let cancel_gas = LAMPORTS_PER_SIGNATURE;

    let begin_end_iterations = 2;
    let iterations: u64 = steps_iterations + begin_end_iterations + storage.realloc_iterations;
    let iterations_gas = iterations * LAMPORTS_PER_SIGNATURE;
    let storage_gas = storage.get_changes_in_rent()?;

    let used_gas = storage_gas + iterations_gas + treasury_gas + cancel_gas;

    let solana_accounts = storage
        .used_accounts()
        .iter()
        .map(|v| SolanaAccount {
            pubkey: v.pubkey,
            is_writable: v.is_writable,
            is_legacy: v.is_legacy,
        })
        .collect::<Vec<_>>();

    let mut result = (
        EmulateResponse {
            exit_status: exit_status.to_string(),
            external_solana_call: execute_status.external_solana_call,
            reverts_before_solana_calls: execute_status.reverts_before_solana_calls,
            reverts_after_solana_calls: execute_status.reverts_after_solana_calls,
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

async fn emulate_trx<'rpc, T: Tracer>(
    emulate_request: &EmulateRequest,
    db_config: &Option<DbConfig>,
    program_id: &Pubkey,
    step_limit: u64,
    tracer: Option<T>,
    rpc: &(impl Rpc + BuildConfigSimulator),
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    info!("tx_params: {:?}", emulate_request.tx);

    if emulate_request.execution_map.is_none() {
        let (mut storage, tx) =
            initialize_storage_and_transaction(program_id, emulate_request, rpc).await?;

        let mut result =
            emulate_trx_single_step(&mut storage, &tx, tracer, emulate_request, step_limit).await?;

        if storage.is_timestamp_used() {
            let mut storage2 =
                initialize_storage_from_other(&storage, 5, 3, emulate_request.tx.chain_id).await?;

            let result2 = emulate_trx_single_step(
                &mut storage2,
                &tx,
                Option::<T>::None,
                emulate_request,
                step_limit,
            )
            .await?;

            let response = &result.0;
            let response2 = &result2.0;

            let mut combined_solana_accounts = response.solana_accounts.clone();
            response2.solana_accounts.iter().for_each(|v| {
                if let Some(w) = combined_solana_accounts
                    .iter_mut()
                    .find(|x| x.pubkey == v.pubkey)
                {
                    w.is_writable |= v.is_writable;
                    w.is_legacy |= v.is_legacy;
                } else {
                    combined_solana_accounts.push(v.clone());
                }
            });

            result.0 = EmulateResponse {
                // We get the result from the first response (as it is executed on the current time)
                result: response.result.clone(),
                exit_status: response.exit_status.to_string(),
                external_solana_call: response.external_solana_call,
                reverts_before_solana_calls: response.reverts_before_solana_calls,
                reverts_after_solana_calls: response.reverts_after_solana_calls,
                accounts_data: None,

                // ...and consumed resources from the both responses (because the real execution can occur in the future)
                steps_executed: response.steps_executed.max(response2.steps_executed),
                used_gas: response.used_gas.max(response2.used_gas),
                iterations: response.iterations.max(response2.iterations),
                solana_accounts: combined_solana_accounts,
                logs: response.logs.clone(),
            };
        }

        return Ok(result);
    }

    emulate_trx_multiple_steps(db_config, program_id, tracer, emulate_request, step_limit).await
}

async fn emulate_trx_single_step<'rpc, T: Tracer>(
    storage: &mut EmulatorAccountStorage<'rpc, impl Rpc + BuildConfigSimulator>,
    tx: &Transaction,
    tracer: Option<T>,
    emulate_request: &EmulateRequest,
    step_limit: u64,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let origin = emulate_request.tx.from;

    let (exit_status, steps_executed, tracer) = {
        let mut backend = SyncedExecutorState::new(storage);
        let mut evm = match Machine::new(tx, origin, &mut backend, tracer).await {
            Ok(evm) => evm,
            Err(e) => {
                error!("EVM creation failed {e:?}");
                return Ok((EmulateResponse::revert(&e, &backend), None));
            }
        };

        let (exit_status, steps_executed, tracer) = evm.execute(step_limit, &mut backend).await?;

        if exit_status == ExitStatus::StepLimit {
            error!("Step_limit={step_limit} exceeded");
            return Ok((
                EmulateResponse::revert(&NeonError::TooManySteps, &backend),
                None,
            ));
        }
        (exit_status, steps_executed, tracer)
    };

    calculate_response(
        steps_executed,
        exit_status,
        storage,
        tracer,
        &emulate_request.provide_account_info,
    )
    .await
}

async fn emulate_trx_multiple_steps<'rpc, T: Tracer>(
    db_config: &Option<DbConfig>,
    program_id: &Pubkey,
    tracer: Option<T>,
    emulate_request: &EmulateRequest,
    step_limit: u64,
) -> NeonResult<(EmulateResponse, Option<Value>)> {
    let execution_map = emulate_request
        .execution_map
        .clone()
        .expect("execution map must be not empty")
        .steps;

    let origin = emulate_request.tx.from;
    let (block, index) = {
        let step = execution_map
            .first()
            .expect("execution map must be not empty")
            .clone();
        (step.block, step.index)
    };

    let mut rpc = create_rpc(db_config, block, index).await?;
    let (mut storage, mut tx) =
        initialize_storage_and_transaction(program_id, emulate_request, &rpc).await?;

    let (exit_status, steps_executed, tracer) = {
        let mut backend = SyncedExecutorState::new(&mut storage);
        let mut evm = match Machine::new(&tx, origin, &mut backend, tracer).await {
            Ok(evm) => evm,
            Err(e) => {
                error!("EVM creation failed {e:?}");
                return Ok((EmulateResponse::revert(&e, &backend), None));
            }
        };

        let mut exit_status: ExitStatus = ExitStatus::Stop;
        let mut steps_executed = 0u64;
        let mut tracer_result: Option<T> = None;
        for execution_step in &execution_map {
            if execution_step.steps == 0 {
                continue;
            }

            if execution_step.is_reset {
                drop(evm);
                drop(backend);
                drop(storage);
                drop(rpc);

                rpc = create_rpc(db_config, execution_step.block, execution_step.index).await?;
                (storage, tx) =
                    initialize_storage_and_transaction(program_id, emulate_request, &rpc).await?;

                backend = SyncedExecutorState::new(&mut storage);
                evm = match Machine::new(&tx, origin, &mut backend, tracer_result).await {
                    Ok(evm) => evm,
                    Err(e) => {
                        error!("EVM creation failed {e:?}");
                        return Ok((EmulateResponse::revert(&e, &backend), None));
                    }
                };
            } else {
                evm.set_tracer(tracer_result);
            }

            let (local_exit_status, local_steps_executed, local_tracer) = evm
                .execute(u64::from(execution_step.steps), &mut backend)
                .await?;

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

        (exit_status, steps_executed, tracer_result)
    };

    calculate_response(
        steps_executed,
        exit_status,
        &storage,
        tracer,
        &emulate_request.provide_account_info,
    )
    .await
}

async fn provide_account_data(
    storage: &EmulatorAccountStorage<'_, impl Rpc>,
    solana_accounts: &[SolanaAccount],
    level: &AccountInfoLevel,
) -> NeonResult<Vec<AccountData>> {
    let pubkeys = solana_accounts
        .iter()
        .filter_map(|v| {
            if v.is_writable || AccountInfoLevel::Changed != *level {
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
            account.map(|acc| AccountData::new_from_account(*pubkey, &acc))
        })
        .collect::<Vec<_>>())
}
