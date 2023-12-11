use crate::tracing::tracers::openeth::tracer::OpenEthereumTracer;
use crate::tracing::tracers::prestate_tracer::tracer::PrestateTracer;
use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
use enum_dispatch::enum_dispatch;
use ethnum::U256;
use evm_loader::evm::tracing::{Event, EventListener, TracerType};
use evm_loader::types::Address;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use web3::types::{Bytes, H256};

pub mod openeth;
pub mod prestate_tracer;
pub mod state_diff;
pub mod struct_logger;

pub fn new_tracer(
    gas_used: Option<U256>,
    trace_config: TraceConfig,
) -> evm_loader::error::Result<TracerType<TracerEnum>> {
    Ok(Rc::new(RefCell::new(build_tracer(gas_used, trace_config)?)))
}

fn build_tracer(
    gas_used: Option<U256>,
    trace_config: TraceConfig,
) -> evm_loader::error::Result<TracerEnum> {
    match trace_config.tracer.as_deref() {
        None | Some("") => Ok(TracerEnum::StructLogger(StructLogger::new(
            gas_used,
            trace_config,
        ))),
        Some("openethereum") => Ok(TracerEnum::OpenEthereumTracer(OpenEthereumTracer::new(
            trace_config,
        ))),
        Some("prestateTracer") => Ok(TracerEnum::PrestateTracer(PrestateTracer::new(
            trace_config,
        ))),
        _ => Err(evm_loader::error::Error::Custom(format!(
            "Unsupported tracer: {:?}",
            trace_config.tracer
        ))),
    }
}

#[enum_dispatch(EventListener, IntoTraces)]
#[derive(Debug)]
pub enum TracerEnum {
    OpenEthereumTracer,
    PrestateTracer,
    StructLogger,
}

// TODO: use enum_dispatch
impl EventListener for TracerEnum {
    fn event(&mut self, event: Event) {
        match self {
            TracerEnum::OpenEthereumTracer(open_ethereum_tracer) => {
                open_ethereum_tracer.event(event)
            }
            TracerEnum::PrestateTracer(prestate_tracer) => prestate_tracer.event(event),
            TracerEnum::StructLogger(struct_logger) => struct_logger.event(event),
        }
    }
}

#[enum_dispatch]
pub trait IntoTraces {
    fn into_traces(self, emulation_result: EmulationResult) -> Value;
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L39>
pub type State = BTreeMap<Address, Account>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L41>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub balance: Option<web3::types::U256>,
    pub code: Option<Bytes>,
    pub nonce: Option<u64>,
    pub storage: Option<BTreeMap<H256, H256>>,
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L255>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct States {
    pub post: State,
    pub pre: State,
}

#[derive(Debug, Clone)]
pub struct EmulationResult {
    pub used_gas: u64,
    pub states: States,
}
