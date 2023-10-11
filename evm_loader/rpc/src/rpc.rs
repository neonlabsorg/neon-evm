use crate::error::NeonRPCError;
use crate::handlers::{
    cancel_trx, collect_treasury, create_ether_account, deposit, get_neon_elf, get_storage_at,
    info, init_environment,
};
use crate::{
    context::Context,
    handlers::{emulate, get_ether_account_data, trace},
};

use jsonrpc_v2::{Data, MapRouter, Server};
use neon_lib::LibMethods;
use std::sync::Arc;

pub fn build_rpc(ctx: Context) -> Result<Arc<Server<MapRouter>>, NeonRPCError> {
    let mut rpc_builder = Server::new().with_data(Data::new(ctx));

    rpc_builder = rpc_builder.with_method("build_info", info::handle);

    rpc_builder = rpc_builder.with_method(
        LibMethods::GetEtherAccountData.to_string(),
        get_ether_account_data::handle,
    );
    rpc_builder =
        rpc_builder.with_method(LibMethods::GetStorageAt.to_string(), get_storage_at::handle);
    rpc_builder = rpc_builder.with_method(LibMethods::Trace.to_string(), trace::handle);
    rpc_builder = rpc_builder.with_method(LibMethods::Emulate.to_string(), emulate::handle);
    rpc_builder = rpc_builder.with_method(LibMethods::CancelTrx.to_string(), cancel_trx::handle);
    rpc_builder = rpc_builder.with_method(
        LibMethods::CollectTreasury.to_string(),
        collect_treasury::handle,
    );
    rpc_builder = rpc_builder.with_method(
        LibMethods::CreateEtherAccount.to_string(),
        create_ether_account::handle,
    );
    rpc_builder = rpc_builder.with_method(LibMethods::Deposit.to_string(), deposit::handle);
    rpc_builder = rpc_builder.with_method(LibMethods::GetNeonElf.to_string(), get_neon_elf::handle);
    rpc_builder = rpc_builder.with_method(
        LibMethods::InitEnvironment.to_string(),
        init_environment::handle,
    );

    let rpc = rpc_builder.finish();

    Ok(rpc)
}
