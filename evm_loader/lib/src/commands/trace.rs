use std::cell::RefCell;
use std::fmt::{Display, Formatter};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use evm_loader::evm::tracing::event_listener::trace::{TraceCallConfig, TraceConfig, TracedCall};
use evm_loader::evm::tracing::event_listener::tracer::Tracer;
use evm_loader::types::Address;

use crate::{
    account_storage::EmulatorAccountStorage, commands::emulate::emulate_trx, errors::NeonError,
    rpc::Rpc, types::TxParams,
};

#[allow(clippy::too_many_arguments)]
pub async fn trace_transaction(
    rpc_client: &dyn Rpc,
    evm_loader: Pubkey,
    tx: TxParams,
    token: Pubkey,
    chain_id: u64,
    steps: u64,
    commitment: CommitmentConfig,
    accounts: &[Address],
    solana_accounts: &[Pubkey],
    trace_call_config: TraceCallConfig,
) -> Result<TracedCall, NeonError> {
    let storage = EmulatorAccountStorage::with_accounts(
        rpc_client,
        evm_loader,
        token,
        chain_id,
        commitment,
        accounts,
        solana_accounts,
        &trace_call_config.block_overrides,
        trace_call_config.state_overrides,
    )
    .await?;

    trace_trx(
        tx,
        &storage,
        chain_id,
        steps,
        &trace_call_config.trace_config,
    )
}

#[derive(Serialize, Deserialize)]
pub struct TraceBlockReturn(pub Vec<TracedCall>);

impl Display for TraceBlockReturn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{ traced call(s): {} }}", self.0.len())
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn trace_block(
    rpc_client: &dyn Rpc,
    evm_loader: Pubkey,
    transactions: Vec<TxParams>,
    token: Pubkey,
    chain_id: u64,
    steps: u64,
    commitment: CommitmentConfig,
    accounts: &[Address],
    solana_accounts: &[Pubkey],
    trace_config: &TraceConfig,
) -> Result<TraceBlockReturn, NeonError> {
    let storage = EmulatorAccountStorage::with_accounts(
        rpc_client,
        evm_loader,
        token,
        chain_id,
        commitment,
        accounts,
        solana_accounts,
        &None,
        None,
    )
    .await?;

    let mut results = vec![];
    for tx_params in transactions {
        let result = trace_trx(tx_params, &storage, chain_id, steps, trace_config)?;
        results.push(result);
    }

    Ok(TraceBlockReturn(results))
}

fn trace_trx<'a>(
    tx_params: TxParams,
    storage: &'a EmulatorAccountStorage<'a>,
    chain_id: u64,
    steps: u64,
    trace_config: &TraceConfig,
) -> Result<TracedCall, NeonError> {
    let tracer = Rc::new(RefCell::new(Some(Tracer::new(
        trace_config.enable_return_data,
    ))));

    let emulation_result = emulate_trx(tx_params, storage, chain_id, steps, Some(tracer.clone()))?;

    let (vm_trace, full_trace_data) = tracer.borrow_mut().take().unwrap().into_traces();

    Ok(TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: emulation_result.used_gas,
        result: emulation_result.result,
        exit_status: emulation_result.exit_status,
    })
}
