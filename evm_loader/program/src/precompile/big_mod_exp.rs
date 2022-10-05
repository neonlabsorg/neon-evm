use std::convert::Infallible;
use std::convert::TryInto;
use evm::{Capture, ExitReason, U256};
use solana_program;

#[must_use]
pub fn big_mod_exp(
    input: &[u8]
) -> Capture<(ExitReason, Vec<u8>), Infallible> {
    // Should be implemented via Solana syscall
    // Capture::Exit((ExitReason::Fatal(evm::ExitFatal::NotSupported), vec![0; 0]));
    #[cfg(target_arch = "bpf")]
    solana_program::log::sol_log_compute_units();

    if input.len() < 96 {
        return Capture::Exit((ExitReason::Succeed(evm::ExitSucceed::Returned), vec![0; 0]))
    };

    let (base_len, rest) = input.split_at(32);
    let (exp_len, rest) = rest.split_at(32);
    let (mod_len, rest) = rest.split_at(32);

    let base_len: usize = match U256::from_big_endian(base_len).try_into() {
        Ok(value) => value,
        Err(_) => return Capture::Exit((ExitReason::Succeed(evm::ExitSucceed::Returned), vec![0; 0]))
    };
    let exp_len: usize = match U256::from_big_endian(exp_len).try_into() {
        Ok(value) => value,
        Err(_) => return Capture::Exit((ExitReason::Succeed(evm::ExitSucceed::Returned), vec![0; 0]))
    };
    let mod_len: usize = match U256::from_big_endian(mod_len).try_into() {
        Ok(value) => value,
        Err(_) => return Capture::Exit((ExitReason::Succeed(evm::ExitSucceed::Returned), vec![0; 0]))
    };

    if base_len == 0 && mod_len == 0 {
        return Capture::Exit((ExitReason::Succeed(evm::ExitSucceed::Returned), vec![0_u8; 32]));
    }

    let (base_val, rest) = rest.split_at(base_len);
    let (exp_val, rest) = rest.split_at(exp_len);
    let (mod_val, _rest) = rest.split_at(mod_len);

    let return_value = solana_program::big_mod_exp::big_mod_exp(base_val, exp_val, mod_val);

    #[cfg(target_arch = "bpf")]
    solana_program::log::sol_log_compute_units();

    Capture::Exit((ExitReason::Succeed(evm::ExitSucceed::Returned), return_value))
}