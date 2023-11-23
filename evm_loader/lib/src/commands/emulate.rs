use evm_loader::account::ContractAccount;
use evm_loader::error::build_revert_message;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use solana_sdk::entrypoint::MAX_PERMITTED_DATA_INCREASE;
use solana_sdk::pubkey::Pubkey;

use crate::syscall_stubs::setup_emulator_syscall_stubs;
use crate::tracing::tracers::prestate_tracer::state_diff::build_states;
use crate::types::{EmulateRequest, TxParams};
use crate::{
    account_storage::{EmulatorAccountStorage, SolanaAccount},
    errors::NeonError,
    rpc::Rpc,
    NeonResult,
};
use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::tracing::{PrestateTracerDiffResult, TracerType};
use evm_loader::{
    config::{EVM_STEPS_MIN, PAYMENT_TO_TREASURE},
    evm::{ExitStatus, Machine},
    executor::{Action, ExecutorState},
    gasometer::LAMPORTS_PER_SIGNATURE,
};
use serde_with::{hex::Hex, serde_as};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulateResponse {
    pub exit_status: String,
    #[serde_as(as = "Hex")]
    pub result: Vec<u8>,
    pub steps_executed: u64,
    pub used_gas: u64,
    pub iterations: u64,
    pub solana_accounts: Vec<SolanaAccount>,
    pub states: PrestateTracerDiffResult,
}

impl EmulateResponse {
    pub fn revert<E: ToString>(e: E) -> Self {
        let revert_message = build_revert_message(&e.to_string());
        let exit_status = ExitStatus::Revert(revert_message);
        Self {
            exit_status: exit_status.to_string(),
            result: exit_status.into_result().unwrap_or_default(),
            steps_executed: 0,
            used_gas: 0,
            iterations: 0,
            solana_accounts: vec![],
            states: Default::default(),
        }
    }
}

pub async fn execute(
    rpc_client: &dyn Rpc,
    program_id: Pubkey,
    emulate_request: EmulateRequest,
    tracer: Option<TracerType>,
) -> NeonResult<EmulateResponse> {
    let block_overrides = emulate_request
        .trace_config
        .as_ref()
        .and_then(|t| t.block_overrides.clone());
    let state_overrides = emulate_request
        .trace_config
        .as_ref()
        .and_then(|t| t.state_overrides.clone());

    let mut storage = EmulatorAccountStorage::with_accounts(
        rpc_client,
        program_id,
        &emulate_request.accounts,
        emulate_request.chains,
        block_overrides,
        state_overrides,
    )
    .await?;

    let step_limit = emulate_request.step_limit.unwrap_or(100000);

    setup_emulator_syscall_stubs(rpc_client).await?;
    let mut backend = ExecutorState::new(&mut storage);
    emulate_trx(emulate_request.tx, &mut backend, step_limit, tracer).await
}

pub async fn emulate_trx(
    tx_params: TxParams,
    backend: &mut ExecutorState<'_, EmulatorAccountStorage<'_>>,
    step_limit: u64,
    tracer: Option<TracerType>,
) -> NeonResult<EmulateResponse> {
    info!("tx_params: {:?}", tx_params);

    let tx_fee = tx_params.gas_used.unwrap_or_default() * tx_params.gas_price.unwrap_or_default();
    let chain_id = tx_params
        .chain_id
        .unwrap_or_else(|| backend.backend.default_chain_id());

    let (origin, tx) = tx_params.into_transaction(backend.backend).await;

    info!("origin: {:?}", origin);
    info!("tx: {:?}", tx);

    let mut evm = match Machine::new(tx, origin, backend, tracer).await {
        Ok(evm) => evm,
        Err(e) => return Ok(EmulateResponse::revert(e)),
    };

    let (exit_status, steps_executed) = evm.execute(step_limit, backend).await?;
    if exit_status == ExitStatus::StepLimit {
        return Err(NeonError::TooManySteps);
    }

    let actions = backend.actions();

    backend.backend.apply_actions(actions.clone()).await?;
    backend.backend.mark_legacy_accounts().await?;

    debug!("Execute done, result={exit_status:?}");
    debug!("{steps_executed} steps executed");

    let steps_iterations = (steps_executed + (EVM_STEPS_MIN - 1)) / EVM_STEPS_MIN;
    let treasury_gas = steps_iterations * PAYMENT_TO_TREASURE;
    let cancel_gas = LAMPORTS_PER_SIGNATURE;

    let begin_end_iterations = 2;
    let iterations: u64 = steps_iterations + begin_end_iterations + realloc_iterations(&actions);
    let iterations_gas = iterations * LAMPORTS_PER_SIGNATURE;

    let used_gas = backend.backend.gas + iterations_gas + treasury_gas + cancel_gas;

    let solana_accounts = backend
        .backend
        .accounts
        .borrow()
        .values()
        .cloned()
        .collect();

    Ok(EmulateResponse {
        exit_status: exit_status.to_string(),
        steps_executed,
        used_gas,
        solana_accounts,
        result: exit_status.into_result().unwrap_or_default(),
        iterations,
        states: build_states(backend, origin, tx_fee, chain_id).await?,
    })
}

fn realloc_iterations(actions: &[Action]) -> u64 {
    let mut result = 0;

    for action in actions {
        if let Action::EvmSetCode { code, .. } = action {
            let size = ContractAccount::required_account_size(code);
            let c = size / MAX_PERMITTED_DATA_INCREASE;
            result = std::cmp::max(result, c);
        }
    }

    result as u64
}
