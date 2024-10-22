use std::convert::TryInto;

use arrayref::array_ref;
use ethnum::U256;
use maybe_async::maybe_async;

use crate::{
    error::{Error, Result},
    evm::database::Database,
    types::{vector::VectorSliceExt, Address, Vector},
};

//-------------------------------------------
// NeonAccount method current ids:
// "5131b14f": "isSolanaUser(address)",
// "b2aebe3c": "solanaAddress(address)"

#[maybe_async]
pub async fn neon_account<State: Database>(
    state: &State,
    address: &Address,
    input: &[u8],
    context: &crate::evm::Context,
    _is_static: bool,
) -> Result<Vector<u8>> {
    debug_print!("neon_account({})", hex::encode(input));

    if context.value != 0 {
        return Err(Error::Custom("Neon Account: value != 0".to_string()));
    }

    let (selector, input) = input.split_at(4);
    let selector: [u8; 4] = selector.try_into()?;

    match selector {
        [0x51, 0x31, 0xb1, 0x4f] => {
            // isSolanaUser(address)
            let address = Address::from(*array_ref![input, 12, 20]);
            is_solana_user(state, address).await
        }
        [0xb2, 0xae, 0xbe, 0x3c] => {
            // solanaAddress(address)
            let address = Address::from(*array_ref![input, 12, 20]);
            solana_address(state, address).await
        }
        _ => {
            debug_print!("neon_account UNKNOWN {:?}", selector);
            Err(Error::UnknownPrecompileMethodSelector(*address, selector))
        }
    }
}

#[maybe_async]
async fn is_solana_user<State: Database>(state: &State, address: Address) -> Result<Vector<u8>> {
    let pubkey = state.solana_user_address(address).await?;
    let result = if pubkey.is_some() {
        U256::ONE.to_be_bytes()
    } else {
        U256::ZERO.to_be_bytes()
    };

    Ok(result.to_vector())
}

#[maybe_async]
async fn solana_address<State: Database>(state: &State, address: Address) -> Result<Vector<u8>> {
    let pubkey = state.solana_user_address(address).await?;
    let result = pubkey.unwrap_or_default().to_bytes();

    Ok(result.to_vector())
}
