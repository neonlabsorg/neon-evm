use evm_loader::account::ContractAccount;
use evm_loader::error::build_revert_message;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use solana_sdk::entrypoint::MAX_PERMITTED_DATA_INCREASE;
use solana_sdk::pubkey::Pubkey;

use crate::rpc::RpcEnum;
use crate::syscall_stubs::setup_emulator_syscall_stubs;
use crate::types::{EmulateRequest, TxParams};
use crate::{
    account_storage::{EmulatorAccountStorage, SolanaAccount},
    errors::NeonError,
    NeonResult,
};
use evm_loader::evm::tracing::TracerType;
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
        }
    }
}

pub async fn execute(
    rpc: &RpcEnum,
    program_id: Pubkey,
    config: EmulateRequest,
    tracer: Option<TracerType>,
) -> NeonResult<EmulateResponse> {
    let block_overrides = config
        .trace_config
        .as_ref()
        .and_then(|t| t.block_overrides.clone());
    let state_overrides = config
        .trace_config
        .as_ref()
        .and_then(|t| t.state_overrides.clone());

    let mut storage = EmulatorAccountStorage::with_accounts(
        rpc,
        program_id,
        &config.accounts,
        config.chains,
        block_overrides,
        state_overrides,
    )
    .await?;

    let step_limit = config.step_limit.unwrap_or(100000);

    setup_emulator_syscall_stubs(rpc).await?;
    emulate_trx(config.tx, &mut storage, step_limit, tracer).await
}

async fn emulate_trx(
    tx_params: TxParams,
    storage: &mut EmulatorAccountStorage<'_>,
    step_limit: u64,
    tracer: Option<TracerType>,
) -> NeonResult<EmulateResponse> {
    info!("tx_params: {:?}", tx_params);

    let (origin, tx) = tx_params.into_transaction(storage).await;

    info!("origin: {:?}", origin);
    info!("tx: {:?}", tx);

    let (exit_status, actions, steps_executed) = {
        let mut backend = ExecutorState::new(storage);
        let mut evm = match Machine::new(tx, origin, &mut backend, tracer).await {
            Ok(evm) => evm,
            Err(e) => return Ok(EmulateResponse::revert(e)),
        };

        let (result, steps_executed) = evm.execute(step_limit, &mut backend).await?;
        if result == ExitStatus::StepLimit {
            return Err(NeonError::TooManySteps);
        }

        let actions = backend.into_actions();
        (result, actions, steps_executed)
    };

    storage.apply_actions(actions.clone()).await?;
    storage.mark_legacy_accounts().await?;

    debug!("Execute done, result={exit_status:?}");
    debug!("{steps_executed} steps executed");

    let steps_iterations = (steps_executed + (EVM_STEPS_MIN - 1)) / EVM_STEPS_MIN;
    let treasury_gas = steps_iterations * PAYMENT_TO_TREASURE;
    let cancel_gas = LAMPORTS_PER_SIGNATURE;

    let begin_end_iterations = 2;
    let iterations: u64 = steps_iterations + begin_end_iterations + realloc_iterations(&actions);
    let iterations_gas = iterations * LAMPORTS_PER_SIGNATURE;

    let used_gas = storage.gas + iterations_gas + treasury_gas + cancel_gas;

    let solana_accounts = storage.accounts.borrow().values().cloned().collect();

    Ok(EmulateResponse {
        exit_status: exit_status.to_string(),
        steps_executed,
        used_gas,
        solana_accounts,
        result: exit_status.into_result().unwrap_or_default(),
        iterations,
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
