use crate::error::Result;
use crate::evm::{database::Database, tracing::EventListener};
use maybe_async::maybe_async;

use super::{opcode::Action, Machine};

struct OpcodeTable<B, T> {
    phantom_database: std::marker::PhantomData<B>,
    phantom_listener: std::marker::PhantomData<T>,
}

macro_rules! opcode_table {
    ($($opcode:literal, $opname:ident, $op:path;)*) => {
        #[cfg(target_os = "solana")]
        type OpCode<B, T> = fn(&mut Machine<T>, &mut B) -> Result<Action>;

        #[cfg(target_os = "solana")]
        impl<B: Database, T: EventListener> OpcodeTable<B, T> {
            const OPCODES: [OpCode<B, T>; 256] = {
                let mut opcodes: [OpCode<B, T>; 256] = [Machine::<T>::opcode_unknown; 256];

                $(opcodes[$opcode as usize] = $op;)*

                opcodes
            };

            pub fn execute_opcode(machine: &mut Machine::<T>, backend: &mut B, opcode: u8) -> Result<Action> {
                // SAFETY: OPCODES.len() == 256, opcode <= 255
                let opcode_fn = unsafe { Self::OPCODES.get_unchecked(opcode as usize) };
                opcode_fn(machine, backend)
            }
        }

        #[cfg(not(target_os = "solana"))]
        impl<B: Database, T: EventListener> OpcodeTable<B, T> {
            pub async fn execute_opcode(machine: &mut Machine::<T>, backend: &mut B, opcode: u8) -> Result<Action> {
                match opcode {
                    $($opcode => $op(machine, backend).await,)*
                    _ => Machine::<T>::opcode_unknown(machine, backend).await,
                }
            }
        }

        #[cfg(not(target_os = "solana"))]
        pub const OPNAMES: [&str; 256] = {
            let mut opnames: [&str; 256] = ["<invalid>"; 256];

            $(opnames[$opcode as usize] = stringify!($opname);)*

            opnames
        };

        #[repr(transparent)]
        #[derive(PartialEq, Eq, Default)]
        pub struct Opcode(pub u8);

        #[cfg(not(target_os = "solana"))]
        $(pub const $opname: Opcode = Opcode($opcode);)*

        #[cfg(not(target_os = "solana"))]
        impl From<u8> for Opcode {
            fn from(opcode: u8) -> Self {
                Opcode(opcode)
            }
        }

        #[cfg(not(target_os = "solana"))]
        impl serde::Serialize for Opcode {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(OPNAMES[self.0 as usize])
            }
        }
    }
}

#[maybe_async]
impl<T: EventListener> Machine<T> {
    pub async fn execute_opcode(
        &mut self,
        backend: &mut impl Database,
        opcode: u8,
    ) -> Result<Action> {
        OpcodeTable::execute_opcode(self, backend, opcode).await
    }
}

opcode_table![
        0x00, STOP, Machine::<T>::opcode_stop;
        0x01, ADD, Machine::<T>::opcode_add;
        0x02, MUL, Machine::<T>::opcode_mul;
        0x03, SUB, Machine::<T>::opcode_sub;
        0x04, DIV, Machine::<T>::opcode_div;
        0x05, SDIV, Machine::<T>::opcode_sdiv;
        0x06, MOD, Machine::<T>::opcode_mod;
        0x07, SMOD, Machine::<T>::opcode_smod;
        0x08, ADDMOD, Machine::<T>::opcode_addmod;
        0x09, MULMOD, Machine::<T>::opcode_mulmod;
        0x0A, EXP, Machine::<T>::opcode_exp;
        0x0B, SIGNEXTEND, Machine::<T>::opcode_signextend;

        0x10, LT, Machine::<T>::opcode_lt;
        0x11, GT, Machine::<T>::opcode_gt;
        0x12, SLT, Machine::<T>::opcode_slt;
        0x13, SGT, Machine::<T>::opcode_sgt;
        0x14, EQ, Machine::<T>::opcode_eq;
        0x15, ISZERO, Machine::<T>::opcode_iszero;
        0x16, AND, Machine::<T>::opcode_and;
        0x17, OR, Machine::<T>::opcode_or;
        0x18, XOR, Machine::<T>::opcode_xor;
        0x19, NOT, Machine::<T>::opcode_not;
        0x1A, BYTE, Machine::<T>::opcode_byte;
        0x1B, SHL, Machine::<T>::opcode_shl;
        0x1C, SHR, Machine::<T>::opcode_shr;
        0x1D, SAR, Machine::<T>::opcode_sar;

        0x20, KECCAK256, Machine::<T>::opcode_sha3;

        0x30, ADDRESS, Machine::<T>::opcode_address;
        0x31, BALANCE, Machine::<T>::opcode_balance;
        0x32, ORIGIN, Machine::<T>::opcode_origin;
        0x33, CALLER, Machine::<T>::opcode_caller;
        0x34, CALLVALUE, Machine::<T>::opcode_callvalue;
        0x35, CALLDATALOAD, Machine::<T>::opcode_calldataload;
        0x36, CALLDATASIZE, Machine::<T>::opcode_calldatasize;
        0x37, CALLDATACOPY, Machine::<T>::opcode_calldatacopy;
        0x38, CODESIZE, Machine::<T>::opcode_codesize;
        0x39, CODECOPY, Machine::<T>::opcode_codecopy;
        0x3A, GASPRICE, Machine::<T>::opcode_gasprice;
        0x3B, EXTCODESIZE, Machine::<T>::opcode_extcodesize;
        0x3C, EXTCODECOPY, Machine::<T>::opcode_extcodecopy;
        0x3D, RETURNDATASIZE, Machine::<T>::opcode_returndatasize;
        0x3E, RETURNDATACOPY, Machine::<T>::opcode_returndatacopy;
        0x3F, EXTCODEHASH, Machine::<T>::opcode_extcodehash;
        0x40, BLOCKHASH, Machine::<T>::opcode_blockhash;
        0x41, COINBASE, Machine::<T>::opcode_coinbase;
        0x42, TIMESTAMP, Machine::<T>::opcode_timestamp;
        0x43, NUMBER, Machine::<T>::opcode_number;
        0x44, PREVRANDAO, Machine::<T>::opcode_difficulty;
        0x45, GASLIMIT, Machine::<T>::opcode_gaslimit;
        0x46, CHAINID, Machine::<T>::opcode_chainid;
        0x47, SELFBALANCE, Machine::<T>::opcode_selfbalance;
        0x48, BASEFEE, Machine::<T>::opcode_basefee;

        0x50, POP, Machine::<T>::opcode_pop;
        0x51, MLOAD, Machine::<T>::opcode_mload;
        0x52, MSTORE, Machine::<T>::opcode_mstore;
        0x53, MSTORE8, Machine::<T>::opcode_mstore8;
        0x54, SLOAD, Machine::<T>::opcode_sload;
        0x55, SSTORE, Machine::<T>::opcode_sstore;
        0x56, JUMP, Machine::<T>::opcode_jump;
        0x57, JUMPI, Machine::<T>::opcode_jumpi;
        0x58, PC, Machine::<T>::opcode_pc;
        0x59, MSIZE, Machine::<T>::opcode_msize;
        0x5A, GAS, Machine::<T>::opcode_gas;
        0x5B, JUMPDEST, Machine::<T>::opcode_jumpdest;

        0x5C, TLOAD, Machine::<T>::opcode_tload;
        0x5D, TSTORE, Machine::<T>::opcode_tstore;
        0x5E, MCOPY, Machine::<T>::opcode_mcopy;

        0x5F, PUSH0, Machine::<T>::opcode_push_0;
        0x60, PUSH1, Machine::<T>::opcode_push_1;
        0x61, PUSH2, Machine::<T>::opcode_push_2_31::<2>;
        0x62, PUSH3, Machine::<T>::opcode_push_2_31::<3>;
        0x63, PUSH4, Machine::<T>::opcode_push_2_31::<4>;
        0x64, PUSH5, Machine::<T>::opcode_push_2_31::<5>;
        0x65, PUSH6, Machine::<T>::opcode_push_2_31::<6>;
        0x66, PUSH7, Machine::<T>::opcode_push_2_31::<7>;
        0x67, PUSH8, Machine::<T>::opcode_push_2_31::<8>;
        0x68, PUSH9, Machine::<T>::opcode_push_2_31::<9>;
        0x69, PUSH10, Machine::<T>::opcode_push_2_31::<10>;
        0x6A, PUSH11, Machine::<T>::opcode_push_2_31::<11>;
        0x6B, PUSH12, Machine::<T>::opcode_push_2_31::<12>;
        0x6C, PUSH13, Machine::<T>::opcode_push_2_31::<13>;
        0x6D, PUSH14, Machine::<T>::opcode_push_2_31::<14>;
        0x6E, PUSH15, Machine::<T>::opcode_push_2_31::<15>;
        0x6F, PUSH16, Machine::<T>::opcode_push_2_31::<16>;
        0x70, PUSH17, Machine::<T>::opcode_push_2_31::<17>;
        0x71, PUSH18, Machine::<T>::opcode_push_2_31::<18>;
        0x72, PUSH19, Machine::<T>::opcode_push_2_31::<19>;
        0x73, PUSH20, Machine::<T>::opcode_push_2_31::<20>;
        0x74, PUSH21, Machine::<T>::opcode_push_2_31::<21>;
        0x75, PUSH22, Machine::<T>::opcode_push_2_31::<22>;
        0x76, PUSH23, Machine::<T>::opcode_push_2_31::<23>;
        0x77, PUSH24, Machine::<T>::opcode_push_2_31::<24>;
        0x78, PUSH25, Machine::<T>::opcode_push_2_31::<25>;
        0x79, PUSH26, Machine::<T>::opcode_push_2_31::<26>;
        0x7A, PUSH27, Machine::<T>::opcode_push_2_31::<27>;
        0x7B, PUSH28, Machine::<T>::opcode_push_2_31::<28>;
        0x7C, PUSH29, Machine::<T>::opcode_push_2_31::<29>;
        0x7D, PUSH30, Machine::<T>::opcode_push_2_31::<30>;
        0x7E, PUSH31, Machine::<T>::opcode_push_2_31::<31>;
        0x7F, PUSH32, Machine::<T>::opcode_push_32;

        0x80, DUP1, Machine::<T>::opcode_dup_1_16::<1>;
        0x81, DUP2, Machine::<T>::opcode_dup_1_16::<2>;
        0x82, DUP3, Machine::<T>::opcode_dup_1_16::<3>;
        0x83, DUP4, Machine::<T>::opcode_dup_1_16::<4>;
        0x84, DUP5, Machine::<T>::opcode_dup_1_16::<5>;
        0x85, DUP6, Machine::<T>::opcode_dup_1_16::<6>;
        0x86, DUP7, Machine::<T>::opcode_dup_1_16::<7>;
        0x87, DUP8, Machine::<T>::opcode_dup_1_16::<8>;
        0x88, DUP9, Machine::<T>::opcode_dup_1_16::<9>;
        0x89, DUP10, Machine::<T>::opcode_dup_1_16::<10>;
        0x8A, DUP11, Machine::<T>::opcode_dup_1_16::<11>;
        0x8B, DUP12, Machine::<T>::opcode_dup_1_16::<12>;
        0x8C, DUP13, Machine::<T>::opcode_dup_1_16::<13>;
        0x8D, DUP14, Machine::<T>::opcode_dup_1_16::<14>;
        0x8E, DUP15, Machine::<T>::opcode_dup_1_16::<15>;
        0x8F, DUP16, Machine::<T>::opcode_dup_1_16::<16>;

        0x90, SWAP1, Machine::<T>::opcode_swap_1_16::<1>;
        0x91, SWAP2, Machine::<T>::opcode_swap_1_16::<2>;
        0x92, SWAP3, Machine::<T>::opcode_swap_1_16::<3>;
        0x93, SWAP4, Machine::<T>::opcode_swap_1_16::<4>;
        0x94, SWAP5, Machine::<T>::opcode_swap_1_16::<5>;
        0x95, SWAP6, Machine::<T>::opcode_swap_1_16::<6>;
        0x96, SWAP7, Machine::<T>::opcode_swap_1_16::<7>;
        0x97, SWAP8, Machine::<T>::opcode_swap_1_16::<8>;
        0x98, SWAP9, Machine::<T>::opcode_swap_1_16::<9>;
        0x99, SWAP10, Machine::<T>::opcode_swap_1_16::<10>;
        0x9A, SWAP11, Machine::<T>::opcode_swap_1_16::<11>;
        0x9B, SWAP12, Machine::<T>::opcode_swap_1_16::<12>;
        0x9C, SWAP13, Machine::<T>::opcode_swap_1_16::<13>;
        0x9D, SWAP14, Machine::<T>::opcode_swap_1_16::<14>;
        0x9E, SWAP15, Machine::<T>::opcode_swap_1_16::<15>;
        0x9F, SWAP16, Machine::<T>::opcode_swap_1_16::<16>;

        0xA0, LOG0, Machine::<T>::opcode_log_0_4::<0>;
        0xA1, LOG1, Machine::<T>::opcode_log_0_4::<1>;
        0xA2, LOG2, Machine::<T>::opcode_log_0_4::<2>;
        0xA3, LOG3, Machine::<T>::opcode_log_0_4::<3>;
        0xA4, LOG4, Machine::<T>::opcode_log_0_4::<4>;

        0xF0, CREATE, Machine::<T>::opcode_create;
        0xF1, CALL, Machine::<T>::opcode_call;
        0xF2, CALLCODE, Machine::<T>::opcode_callcode;
        0xF3, RETURN, Machine::<T>::opcode_return;
        0xF4, DELEGATECALL, Machine::<T>::opcode_delegatecall;
        0xF5, CREATE2, Machine::<T>::opcode_create2;

        0xFA, STATICCALL, Machine::<T>::opcode_staticcall;

        0xFD, REVERT, Machine::<T>::opcode_revert;
        0xFE, INVALID, Machine::<T>::opcode_invalid;

        0xFF, SENDALL, Machine::<T>::opcode_sendall;
];
