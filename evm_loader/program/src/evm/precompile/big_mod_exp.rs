use crate::evm::U256;
use dashu::{integer::fast_div::ConstDivisor, integer::UBig};

fn dashu_mod_exp(base: &[u8], exponent: &[u8], modulus: &[u8]) -> Vec<u8> {
    let modulus_len = modulus.len();
    let modulus = ConstDivisor::new(UBig::from_be_bytes(modulus));
    let base = modulus.reduce(UBig::from_be_bytes(base));

    let ret_int = base.pow(&UBig::from_be_bytes(exponent)).residue();
    let ret_int = ret_int.to_be_bytes();
    let mut return_value = vec![0_u8; modulus_len.saturating_sub(ret_int.len())];
    return_value.extend(ret_int);
    return_value
}

#[must_use]
pub fn big_mod_exp(input: &[u8]) -> Vec<u8> {
    if input.len() < 96 {
        return vec![];
    }

    let (base_len, rest) = input.split_at(32);
    let (exp_len, rest) = rest.split_at(32);
    let (mod_len, rest) = rest.split_at(32);

    let Ok(base_len) = U256::from_be_bytes(base_len.try_into().unwrap()).try_into() else {
        return vec![];
    };
    let Ok(exp_len) = U256::from_be_bytes(exp_len.try_into().unwrap()).try_into() else {
        return vec![];
    };
    let Ok(mod_len) = U256::from_be_bytes(mod_len.try_into().unwrap()).try_into() else {
        return vec![];
    };

    if base_len == 0 || mod_len == 0 {
        return vec![0; 32];
    }

    let (base_val, rest) = rest.split_at(base_len);
    let (exp_val, rest) = rest.split_at(exp_len);
    let (mod_val, _) = rest.split_at(mod_len);

    dashu_mod_exp(base_val, exp_val, mod_val)
}
