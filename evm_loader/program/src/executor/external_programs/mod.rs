#![allow(clippy::large_enum_variant)] // it's boxed at use site

use enum_dispatch::enum_dispatch;
use maybe_async::maybe_async;
use solana_program::pubkey::Pubkey;

use crate::error::Result;
use crate::{executor::OwnedAccountInfo, platform::Platform, types::vector::VectorMap};

pub mod metaplex;
pub mod spl_associated_token;
pub mod spl_token;
pub mod system;

pub use metaplex::{Metadata, Metaplex};
pub use spl_associated_token::{get_associated_token_address, SplAssociatedToken};
pub use spl_token::SplToken;
pub use system::SystemProgram;

use metaplex::{CreateMasterEdition, CreateMetadata};
use spl_associated_token::CreateAssociatedToken;
use spl_token::{
    Approve, Burn, CloseAccount, Freeze, InitializeAccount, InitializeMint, MintTo, Revoke, Thaw,
    Transfer,
};
use system::CreateAccount;

type AccountsMap = VectorMap<Pubkey, OwnedAccountInfo>;

#[maybe_async(?Send)]
#[enum_dispatch(SystemProgram)]
#[enum_dispatch(Metaplex)]
#[enum_dispatch(SplToken)]
#[enum_dispatch(SplAssociatedToken)]
#[enum_dispatch(ExternalProgram)]
pub trait Invokable {
    fn for_each_mutable_account<'a>(&'a self, f: impl FnMut(&'a Pubkey));

    async fn emulate(&self, platform: &impl Platform, accounts: &mut AccountsMap) -> Result<()>;
    async fn invoke(&self, platform: &mut impl Platform) -> Result<()>;
}

#[enum_dispatch]
pub enum ExternalProgram {
    SystemProgram,
    Metaplex,
    SplToken,
    SplAssociatedToken,
}
