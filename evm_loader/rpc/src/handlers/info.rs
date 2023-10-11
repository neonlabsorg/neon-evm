use serde_json::json;

use crate::build_info::get_build_info;

pub async fn handle() -> Result<serde_json::Value, jsonrpc_v2::Error> {
    Ok(json!(get_build_info()))
}
