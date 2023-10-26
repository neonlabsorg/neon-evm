pub mod cancel_trx;
pub mod collect_treasury;
pub mod create_ether_account;
pub mod deposit;
pub mod emulate;
pub mod get_ether_account_data;
pub mod get_neon_elf;
pub mod get_storage_at;
pub mod info;
pub mod init_environment;
pub mod trace;

use crate::context::Context;
use jsonrpc_v2::Data;
use neon_lib::LibMethods;
use neon_lib_interface::types::NeonEVMLibError;
use serde::Serialize;
use serde_json::Value;

pub async fn invoke(
    method: LibMethods,
    context: Data<Context>,
    params: impl Serialize,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    // just for testing
    let hash = context
        .libraries
        .keys()
        .last()
        .ok_or(jsonrpc_v2::Error::internal("library collection is empty"))?;

    let library = context
        .libraries
        .get(hash)
        .ok_or(jsonrpc_v2::Error::internal(format!(
            "Library not found for hash {hash}"
        )))?;

    tracing::debug!("ver {:?}", library.hash()());

    let method_str: &str = method.into();

    library.invoke()(
        method_str.into(),
        serde_json::to_string(&params).unwrap().as_str().into(),
    )
    .await
    .map(|x| serde_json::from_str::<serde_json::Value>(&x).unwrap())
    .map_err(|s| {
        let NeonEVMLibError {
            code,
            message,
            data,
        } = serde_json::from_str(s.as_str()).unwrap();

        jsonrpc_v2::Error::Full {
            code: code as i64,
            message,
            data: Some(Box::new(
                data.as_ref()
                    .and_then(Value::as_str)
                    .unwrap_or("null")
                    .to_string(),
            )),
        }
    })
    .into()
}
