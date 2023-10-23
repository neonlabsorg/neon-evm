use std::rc::Rc;

use evm_loader::executor::ExecutorState;
use serde_json::Value;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use crate::account_storage::EmulatorAccountStorage;
use crate::commands::emulate::emulate_trx;
use crate::tracing::tracers::new_tracer;
use crate::tracing::TraceCallConfig;
use crate::{errors::NeonError, rpc::Rpc, types::TxParams};

#[allow(clippy::too_many_arguments)]
pub async fn trace_transaction(
    rpc_client: &dyn Rpc,
    evm_loader: Pubkey,
    tx: TxParams,
    token: Pubkey,
    chain_id: u64,
    steps: u64,
    commitment: CommitmentConfig,
    trace_call_config: TraceCallConfig,
) -> Result<Value, NeonError> {
    let storage = EmulatorAccountStorage::with_accounts(
        rpc_client,
        evm_loader,
        token,
        chain_id,
        commitment,
        &trace_call_config.block_overrides,
        trace_call_config.state_overrides,
    )
    .await?;

    let mut backend = ExecutorState::new(&storage);

    let tracer = new_tracer(tx.gas_used, &trace_call_config.trace_config)?;

    let emulation_result = emulate_trx(
        tx,
        &storage,
        chain_id,
        steps,
        Some(Rc::clone(&tracer)),
        &mut backend,
    )
    .await?;

    Ok(Rc::try_unwrap(tracer)
        .expect("There is must be only one reference")
        .into_inner()
        .into_traces(emulation_result))
}
