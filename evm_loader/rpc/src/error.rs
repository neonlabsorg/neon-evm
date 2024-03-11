use neon_lib::errors::NeonError;
use neon_lib_interface::NeonEVMLibLoadError;
use std::net::AddrParseError;

use thiserror::Error;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum NeonRPCError {
    /// Std IO Error
    #[error("Std I/O error. {0:?}")]
    StdIoError(#[from] std::io::Error),
    #[error("Addr parse error. {0:?}")]
    AddrParseError(#[from] AddrParseError),
    #[error("Neon error. {0:?}")]
    NeonError(#[from] NeonError),
    #[error("Neon lib error. {0:?}")]
    NeonEVMLibLoadError(#[from] NeonEVMLibLoadError),
    #[error("Neon RPC: Incorrect parameters.")]
    IncorrectParameters(),
}

impl From<NeonRPCError> for jsonrpc_v2::Error {
    fn from(value: NeonRPCError) -> Self {
        jsonrpc_v2::Error::internal(value)
    }
}
