use crate::context::Context;
use crate::handlers::{
    emulate, get_balance, get_config, get_contract, get_holder, get_storage_at, info, lib_info,
    trace,
};

use jsonrpc_v2::{Data, MapRouter, Server};
use neon_lib::LibMethod;
use std::sync::Arc;

pub fn build_rpc(ctx: Context) -> Arc<Server<MapRouter>> {
    Server::new()
        .with_data(Data::new(ctx))
        .with_method("build_info", info::handle)
        .with_method("lib_build_info", lib_info::handle)
        .with_method(LibMethod::GetStorageAt.to_string(), get_storage_at::handle)
        .with_method(LibMethod::Trace.to_string(), trace::handle)
        .with_method(LibMethod::Emulate.to_string(), emulate::handle)
        .with_method(LibMethod::GetBalance.to_string(), get_balance::handle)
        .with_method(LibMethod::GetConfig.to_string(), get_config::handle)
        .with_method(LibMethod::GetHolder.to_string(), get_holder::handle)
        .with_method(LibMethod::GetContract.to_string(), get_contract::handle)
        .finish()
}
