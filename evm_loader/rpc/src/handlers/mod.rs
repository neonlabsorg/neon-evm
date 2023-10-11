pub mod emulate;
pub mod get_balance;
pub mod get_config;
pub mod get_contract;
pub mod get_holder;
pub mod get_storage_at;
pub mod info;
pub mod lib_info;
pub mod trace;

use crate::context::Context;
use jsonrpc_v2::Data;
use neon_lib::LibMethod;
use neon_lib_interface::{types::NeonEVMLibError, NeonEVMLib_Ref};
use serde::Serialize;
use serde_json::Value;

async fn get_library(context: &Data<Context>) -> Result<&NeonEVMLib_Ref, jsonrpc_v2::Error> {
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

    Ok(library)
}

pub async fn invoke(
    method: LibMethod,
    context: Data<Context>,
    params: Option<impl Serialize>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    let library = get_library(&context).await?;

    let method_str: &str = method.into();
    let mut params_str: String = "".to_string();
    if let Some(params_value) = params {
        params_str = serde_json::to_string(&params_value).unwrap();
    }

    library.invoke()(
        method_str.into(),
        serde_json::to_string(&params_str).unwrap().as_str().into(),
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

pub async fn lib_build_info(
    context: Data<Context>,
) -> Result<serde_json::Value, jsonrpc_v2::Error> {
    let library = get_library(&context).await?;
    let build_info = library.get_build_info()();

    Ok(serde_json::from_str::<serde_json::Value>(&build_info).unwrap())
}
