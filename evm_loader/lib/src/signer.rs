use std::sync::Arc;

use solana_sdk::signer::Signer;

#[derive(Clone)]
pub struct NeonSigner {
    signer: Arc<dyn Signer>,
}

impl NeonSigner {
    pub fn new(signer: Box<dyn Signer>) -> Self {
        NeonSigner {
            signer: Arc::from(signer),
        }
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

unsafe impl Send for NeonSigner {}
unsafe impl Sync for NeonSigner {}

impl Signer for NeonSigner {
    fn pubkey(&self) -> solana_sdk::pubkey::Pubkey {
        self.signer.pubkey()
    }

    fn try_pubkey(&self) -> Result<solana_sdk::pubkey::Pubkey, solana_sdk::signer::SignerError> {
        self.signer.try_pubkey()
    }

    fn sign_message(&self, message: &[u8]) -> solana_sdk::signature::Signature {
        self.signer.sign_message(message)
    }

    fn try_sign_message(
        &self,
        message: &[u8],
    ) -> Result<solana_sdk::signature::Signature, solana_sdk::signer::SignerError> {
        self.signer.try_sign_message(message)
    }

    fn is_interactive(&self) -> bool {
        self.signer.is_interactive()
    }
}

impl std::ops::Deref for NeonSigner {
    type Target = dyn Signer;

    fn deref(&self) -> &Self::Target {
        &*self.signer
    }
}
