use log::info;
use solana_sdk::{program_error::ProgramError, program_stubs::SyscallStubs, sysvar::rent::Rent};
use std::cell::RefCell;

use crate::{errors::NeonError, rpc::Rpc};

pub struct DefaultStubs;
impl SyscallStubs for DefaultStubs {}

thread_local! {
    static USE_ORIGINAL_STUBS: RefCell<bool> = RefCell::new(false);
}
pub fn use_original_stubs_for_thread(new: bool) -> bool {
    USE_ORIGINAL_STUBS.with(|invoke_context| invoke_context.replace(new))
}
pub fn is_original_stubs_for_thread() -> bool {
    USE_ORIGINAL_STUBS.with(|invoke_context| *invoke_context.borrow())
}

pub struct EmulatorStubs {
    rent: Rent,
    original_stubs: Box<dyn SyscallStubs>,
}

impl EmulatorStubs {
    pub async fn new(
        rpc: &impl Rpc,
        original_stubs: Box<dyn SyscallStubs>,
    ) -> Result<Box<EmulatorStubs>, NeonError> {
        let rent_pubkey = solana_sdk::sysvar::rent::id();
        let data = rpc
            .get_account(&rent_pubkey)
            .await?
            .value
            .map(|a| a.data)
            .unwrap_or_default();
        let rent = bincode::deserialize(&data).map_err(|_| ProgramError::InvalidArgument)?;

        Ok(Box::new(Self {
            rent,
            original_stubs,
        }))
    }
}

impl SyscallStubs for EmulatorStubs {
    fn sol_get_rent_sysvar(&self, pointer: *mut u8) -> u64 {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_get_rent_sysvar(pointer)
        } else {
            unsafe {
                #[allow(clippy::cast_ptr_alignment)]
                let rent = pointer.cast::<Rent>();
                *rent = self.rent;
            }
            0
        }
    }

    fn sol_log(&self, message: &str) {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_log(message)
        } else {
            info!("{}", message);
        }
    }

    fn sol_log_data(&self, fields: &[&[u8]]) {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_log_data(fields)
        } else {
            let mut messages: Vec<String> = Vec::new();

            for f in fields {
                if let Ok(str) = String::from_utf8(f.to_vec()) {
                    messages.push(str);
                } else {
                    messages.push(hex::encode(f));
                }
            }

            info!("Program Data: {}", messages.join(" "));
        }
    }

    fn sol_get_clock_sysvar(&self, _var_addr: *mut u8) -> u64 {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_get_clock_sysvar(_var_addr)
        } else {
            DefaultStubs {}.sol_get_clock_sysvar(_var_addr)
        }
    }

    fn sol_get_epoch_schedule_sysvar(&self, _var_addr: *mut u8) -> u64 {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_get_epoch_schedule_sysvar(_var_addr)
        } else {
            DefaultStubs {}.sol_get_epoch_schedule_sysvar(_var_addr)
        }
    }

    fn sol_get_fees_sysvar(&self, _var_addr: *mut u8) -> u64 {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_get_fees_sysvar(_var_addr)
        } else {
            DefaultStubs {}.sol_get_fees_sysvar(_var_addr)
        }
    }

    fn sol_get_return_data(&self) -> Option<(solana_sdk::pubkey::Pubkey, Vec<u8>)> {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_get_return_data()
        } else {
            DefaultStubs {}.sol_get_return_data()
        }
    }

    fn sol_get_stack_height(&self) -> u64 {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_get_stack_height()
        } else {
            DefaultStubs {}.sol_get_stack_height()
        }
    }

    fn sol_invoke_signed(
        &self,
        _instruction: &solana_sdk::instruction::Instruction,
        _account_infos: &[solana_sdk::account_info::AccountInfo],
        _signers_seeds: &[&[&[u8]]],
    ) -> solana_sdk::entrypoint::ProgramResult {
        if is_original_stubs_for_thread() {
            self.original_stubs
                .sol_invoke_signed(_instruction, _account_infos, _signers_seeds)
        } else {
            DefaultStubs {}.sol_invoke_signed(_instruction, _account_infos, _signers_seeds)
        }
    }

    fn sol_log_compute_units(&self) {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_log_compute_units()
        } else {
            DefaultStubs {}.sol_log_compute_units()
        }
    }

    fn sol_set_return_data(&self, _data: &[u8]) {
        if is_original_stubs_for_thread() {
            self.original_stubs.sol_set_return_data(_data)
        } else {
            DefaultStubs {}.sol_set_return_data(_data)
        }
    }
}

pub async fn setup_emulator_syscall_stubs(rpc: &impl Rpc) -> Result<(), NeonError> {
    use solana_sdk::program_stubs::set_syscall_stubs;

    let original_stubs = set_syscall_stubs(Box::new(DefaultStubs {}));
    let syscall_stubs = EmulatorStubs::new(rpc, original_stubs).await?;
    set_syscall_stubs(syscall_stubs);

    Ok(())
}
