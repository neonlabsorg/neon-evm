use ethnum::U256;

use crate::types::vector::VectorVecExt;
use crate::types::Vector;
use crate::vector;

#[must_use]
pub fn big_mod_exp(input: &[u8]) -> Vector<u8> {
    if input.len() < 96 {
        return vector![];
    }

    let (base_len, rest) = input.split_at(32);
    let (exp_len, rest) = rest.split_at(32);
    let (mod_len, rest) = rest.split_at(32);

    let Ok(base_len) = U256::from_be_bytes(base_len.try_into().unwrap()).try_into() else {
        return vector![];
    };
    let Ok(exp_len) = U256::from_be_bytes(exp_len.try_into().unwrap()).try_into() else {
        return vector![];
    };
    let Ok(mod_len) = U256::from_be_bytes(mod_len.try_into().unwrap()).try_into() else {
        return vector![];
    };

    if base_len == 0 && mod_len == 0 {
        return vector![0; 32];
    }

    let (base_val, rest) = rest.split_at(base_len);
    let (exp_val, rest) = rest.split_at(exp_len);
    let (mod_val, _) = rest.split_at(mod_len);

    solana_program::big_mod_exp::big_mod_exp(base_val, exp_val, mod_val).into_vector()
}
