use std::sync::Arc;

use solana_clap_utils::keypair::signer_from_path;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    signer::{Signer, SignerError},
};

use crate::{Config, NeonError};

#[derive(Clone)]
pub struct NeonSigner {
    signer: Arc<dyn Signer>,
}

impl NeonSigner {
    pub fn new(config: &Config) -> Result<Self, NeonError> {
        let mut wallet_manager = None;

        let signer = signer_from_path(
            &Default::default(),
            &config.keypair_path,
            "keypair",
            &mut wallet_manager,
        )
        .map_err(|_| NeonError::KeypairNotSpecified)?;

        let signer = Arc::from(signer);

        Ok(NeonSigner { signer })
    }
}

impl<T> From<Arc<T>> for NeonSigner
where
    T: Signer + 'static,
{
    fn from(value: Arc<T>) -> Self {
        NeonSigner { signer: value }
    }
}

impl From<Arc<dyn Signer>> for NeonSigner {
    fn from(value: Arc<dyn Signer>) -> Self {
        NeonSigner { signer: value }
    }
}

impl Signer for NeonSigner {
    fn pubkey(&self) -> Pubkey {
        self.signer.pubkey()
    }

    fn try_pubkey(&self) -> Result<Pubkey, SignerError> {
        self.signer.try_pubkey()
    }

    fn sign_message(&self, message: &[u8]) -> Signature {
        self.signer.sign_message(message)
    }

    fn try_sign_message(&self, message: &[u8]) -> Result<Signature, SignerError> {
        self.signer.try_sign_message(message)
    }

    fn is_interactive(&self) -> bool {
        self.signer.is_interactive()
    }
}

/// # Safety
/// Every implementation of solana_sdk::signer::Signer should be Send
unsafe impl Send for NeonSigner {}
/// # Safety
/// Every implementation of solana_sdk::signer::Signer should be Sync
unsafe impl Sync for NeonSigner {}

impl std::ops::Deref for NeonSigner {
    type Target = dyn Signer;

    fn deref(&self) -> &Self::Target {
        &*self.signer
    }
}
