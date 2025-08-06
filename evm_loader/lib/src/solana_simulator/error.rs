#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Program Account error")]
    ProgramAccountError,
    #[error("Rpc Client error {0:?}")]
    RpcClientError(#[from] solana_client::client_error::ClientError),
    #[error("Bincode error {0:?}")]
    BincodeError(#[from] bincode::Error),
    #[error("Failed to download sysvar accounts")]
    SysvarError,
    #[error("Failed to extract ELF from account")]
    AccountIsNotProgram,
    #[error("Account {0} is not supported for simulation")]
    UnsupportedAccount(solana_sdk::pubkey::Pubkey),
}
