use allocator_api2::alloc::Allocator;
use maybe_async::maybe_async;

use crate::error::Result;
use crate::evm::tracing::EventListener;
use crate::evm::Machine;
use crate::types::Address;

use super::database::Database;

mod big_mod_exp;
mod blake2_f;
mod bn256;
mod datacopy;
mod ecrecover;
mod ripemd160;
mod sha256;

const SYSTEM_ACCOUNT_ECRECOVER: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01,
]);
const SYSTEM_ACCOUNT_SHA_256: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02,
]);
const SYSTEM_ACCOUNT_RIPEMD160: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x03,
]);
const SYSTEM_ACCOUNT_DATACOPY: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x04,
]);
const SYSTEM_ACCOUNT_BIGMODEXP: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x05,
]);
const SYSTEM_ACCOUNT_BN256_ADD: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x06,
]);
const SYSTEM_ACCOUNT_BN256_SCALAR_MUL: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x07,
]);
const SYSTEM_ACCOUNT_BN256_PAIRING: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x08,
]);
const SYSTEM_ACCOUNT_BLAKE2F: Address = Address([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x09,
]);

/// Is precompile address
#[must_use]
pub fn is_precompile_address(address: &Address) -> bool {
    *address == SYSTEM_ACCOUNT_ECRECOVER
        || *address == SYSTEM_ACCOUNT_SHA_256
        || *address == SYSTEM_ACCOUNT_RIPEMD160
        || *address == SYSTEM_ACCOUNT_DATACOPY
        || *address == SYSTEM_ACCOUNT_BIGMODEXP
        || *address == SYSTEM_ACCOUNT_BN256_ADD
        || *address == SYSTEM_ACCOUNT_BN256_SCALAR_MUL
        || *address == SYSTEM_ACCOUNT_BN256_PAIRING
        || *address == SYSTEM_ACCOUNT_BLAKE2F
}

impl<A, T> Machine<A, T>
where
    A: Allocator + Copy,
    T: EventListener,
{
    #[must_use]
    pub fn standard_precompile(address: &Address, data: &[u8]) -> Option<Vec<u8>> {
        match *address {
            SYSTEM_ACCOUNT_ECRECOVER => Some(ecrecover::ecrecover(data)),
            SYSTEM_ACCOUNT_SHA_256 => Some(sha256::sha256(data)),
            SYSTEM_ACCOUNT_RIPEMD160 => Some(ripemd160::ripemd160(data)),
            SYSTEM_ACCOUNT_DATACOPY => Some(datacopy::datacopy(data)),
            SYSTEM_ACCOUNT_BIGMODEXP => Some(big_mod_exp::big_mod_exp(data)),
            SYSTEM_ACCOUNT_BN256_ADD => Some(bn256::bn256_add(data)),
            SYSTEM_ACCOUNT_BN256_SCALAR_MUL => Some(bn256::bn256_scalar_mul(data)),
            SYSTEM_ACCOUNT_BN256_PAIRING => Some(bn256::bn256_pairing(data)),
            SYSTEM_ACCOUNT_BLAKE2F => Some(blake2_f::blake2_f(data)),
            _ => None,
        }
    }

    #[maybe_async]
    pub async fn try_call_precompile(
        &self,
        address: &Address,
        backend: &mut impl Database,
    ) -> Option<Result<Vec<u8>>> {
        let call_data = self.call_data.as_slice(&self.parent);

        let value = Self::standard_precompile(address, call_data);
        if let Some(value) = value {
            return Some(Ok(value));
        }

        backend
            .precompile_extension(&self.context, address, call_data, self.is_static)
            .await
    }
}
