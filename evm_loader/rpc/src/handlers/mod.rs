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
use serde_json::Value;

pub async fn invoke(
    method: LibMethods,
    context: Data<Context>,
    params: serde_json::Value,
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

    let result: Result<_, _> = library.invoke()(
        method_str.into(),
        serde_json::to_string(&params).unwrap().as_str().into(),
    )
    .await
    .map(|x| serde_json::from_str::<serde_json::Value>(&x).unwrap())
    .map_err(String::from)
    .into();

    result.map_err(|s: String| {
        let val: Value = serde_json::from_str(s.as_str()).unwrap();
        let code = val
            .get("code")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        let message = val
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let data = val
            .get("data")
            .and_then(|value| value.as_str())
            .unwrap_or("null");

        jsonrpc_v2::Error::Full {
            code,
            message: message.to_string(),
            data: Some(Box::new(data.to_string())),
        }
    })
}
