use crate::{
    commands::emulate::{emulate_transaction, emulate_trx, setup_syscall_stubs},
    event_listener::tracer::Tracer,
    errors::NeonCliError,
    rpc::Rpc,
    types::{
        trace::{TracedCall, TraceCallConfig, TraceConfig},
        TxParams,
    },
    account_storage::EmulatorAccountStorage,
};
use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;

#[allow(clippy::too_many_arguments)]
pub fn trace_transaction(
    rpc_client: &dyn Rpc,
    evm_loader: Pubkey,
    tx: TxParams,
    token: Pubkey,
    chain_id: u64,
    steps: u64,
    accounts: &[Address],
    trace_call_config: TraceCallConfig,
) -> Result<TracedCall, NeonCliError> {
    let mut tracer = Tracer::new(trace_call_config.trace_config.enable_return_data);

    let (emulation_result, _storage) = evm_loader::evm::tracing::using(&mut tracer, || {
        emulate_transaction(rpc_client, evm_loader, tx, token, chain_id, steps, accounts, trace_call_config)
    })?;

    let (vm_trace, full_trace_data) = tracer.into_traces();

    Ok(TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: emulation_result.used_gas,
        result: emulation_result.result,
        exit_status: emulation_result.exit_status,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn trace_block(
    rpc_client: &dyn Rpc,
    evm_loader: Pubkey,
    transactions: Vec<TxParams>,
    token: Pubkey,
    chain_id: u64,
    steps: u64,
    accounts: &[Address],
    trace_config: TraceConfig,
) -> Result<Vec<TracedCall>, NeonCliError> {
    setup_syscall_stubs(rpc_client)?;

    let storage = EmulatorAccountStorage::with_accounts(
        rpc_client,
        evm_loader,
        token,
        chain_id,
        None,
        None,
        accounts,
    );

    let mut results = vec![];
    for tx_params in transactions {
        let result = trace_trx(tx_params, &storage, chain_id, steps, trace_config.clone())?;
        results.push(result);
    }

    Ok(results)
}

fn trace_trx(
    tx_params: TxParams,
    storage: &EmulatorAccountStorage,
    chain_id: u64,
    steps: u64,
    trace_config: TraceConfig,
) -> Result<TracedCall, NeonCliError> {
    let mut tracer = Tracer::new(trace_config.enable_return_data);

    let emulation_result = evm_loader::evm::tracing::using(
        &mut tracer,
        || emulate_trx(tx_params, storage, chain_id, steps),
    )?;

    let (vm_trace, full_trace_data) = tracer.into_traces();

    Ok(TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: emulation_result.used_gas,
        result: emulation_result.result,
        exit_status: emulation_result.exit_status,
    })
}
