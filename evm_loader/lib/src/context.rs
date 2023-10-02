use crate::{
    rpc::{self},
    Config, NeonError,
};
use evm_loader::solana_program::pubkey::Pubkey;
use solana_clap_utils::keypair::signer_from_path;
use solana_sdk::signature::Signer;
use std::sync::Arc;

pub struct RequestContext<'a> {
    pub rpc_client: Arc<dyn rpc::Rpc>,
    pub config: &'a Config,
}

impl<'a> RequestContext<'a> {
    pub fn new(
        rpc_client: Arc<dyn rpc::Rpc>,
        config: &'a Config,
    ) -> Result<RequestContext<'a>, NeonError> {
        Ok(Self { rpc_client, config })
    }

    pub fn evm_loader(&self) -> &Pubkey {
        &self.config.evm_loader
    }

    pub fn signer(&self) -> Result<Box<dyn Signer>, NeonError> {
        build_signer(self.config)
    }
}

/// # Errors
pub fn build_signer(config: &Config) -> Result<Box<dyn Signer>, NeonError> {
    let mut wallet_manager = None;

    let signer = signer_from_path(
        &Default::default(),
        &config.keypair_path,
        "keypair",
        &mut wallet_manager,
    )
    .map_err(NeonError::KeypairNotSpecified)?;

    Ok(signer)
}
