use thiserror::Error;

#[derive(Debug, Error)]
pub enum NeonRpcClientError {
    #[error("Jsonrpc error. {0:?}")]
    JsonrpseeError(#[from] jsonrpsee_core::Error),
    #[error("serde json error. {0:?}")]
    SerdeJsonError(#[from] serde_json::Error),
}
