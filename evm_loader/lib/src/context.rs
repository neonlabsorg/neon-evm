use crate::{rpc, rpc::CallDbClient, Config, NeonCliError};
use solana_clap_utils::keypair::signer_from_path;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signer;

pub struct Context {
    pub rpc_client: Box<dyn rpc::Rpc>,
    pub signer: Box<dyn Signer>,
}

#[must_use]
pub fn create(rpc_client: Box<dyn rpc::Rpc>, signer: Box<dyn Signer>) -> Context {
    Context { rpc_client, signer }
}

/// # Errors
pub fn build_signer(config: &Config) -> Result<Box<dyn Signer>, NeonCliError> {
    let mut wallet_manager = None;

    let signer = signer_from_path(
        &Default::default(),
        &config.keypair_path,
        "keypair",
        &mut wallet_manager,
    )
    .map_err(|_| NeonCliError::KeypairNotSpecified)?;

    Ok(signer)
}

/// # Errors
pub fn build_rpc_client(
    config: &Config,
    slot: Option<u64>,
) -> Result<Box<dyn rpc::Rpc>, NeonCliError> {
    if let Some(slot) = slot {
        let config = config
            .db_config
            .clone()
            .ok_or(NeonCliError::InvalidChDbConfig)?;
        return Ok(Box::new(CallDbClient::new(&config, slot)));
    }

    Ok(Box::new(RpcClient::new_with_commitment(
        config.json_rpc_url.clone(),
        config.commitment,
    )))
}
