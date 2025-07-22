use crate::error::Result;
use crate::evm::{database::Database, tracing::EventListener};
use allocator_api2::alloc::Allocator;
use maybe_async::maybe_async;

use super::{opcode::Action, Machine};

struct OpcodeTable<B, A, T> {
    database: std::marker::PhantomData<B>,
    allocator: std::marker::PhantomData<A>,
    listener: std::marker::PhantomData<T>,
}

macro_rules! opcode_table {
    ($($opcode:literal, $opname:ident, $op:path;)*) => {
        #[cfg(target_os = "solana")]
        type OpCode<B, A, T> = fn(&mut Machine<A, T>, &mut B) -> Result<Action>;

        #[cfg(target_os = "solana")]
        impl<B: Database, A: Allocator + Copy, T: EventListener> OpcodeTable<B, A, T> {
            const OPCODES: [OpCode<B, A, T>; 256] = {
                let mut opcodes: [OpCode<B, A, T>; 256] = [Machine::<A, T>::opcode_unknown; 256];

                $(opcodes[$opcode as usize] = $op;)*

                opcodes
            };

            pub fn execute_opcode(machine: &mut Machine::<A, T>, backend: &mut B, opcode: u8) -> Result<Action> {
                // SAFETY: OPCODES.len() == 256, opcode <= 255
                let opcode_fn = unsafe { Self::OPCODES.get_unchecked(opcode as usize) };
                opcode_fn(machine, backend)
            }
        }

        #[cfg(not(target_os = "solana"))]
        impl<B: Database, A: Allocator + Copy, T: EventListener> OpcodeTable<B, A, T> {
            pub async fn execute_opcode(machine: &mut Machine::<A, T>, backend: &mut B, opcode: u8) -> Result<Action> {
                match opcode {
                    $($opcode => $op(machine, backend).await,)*
                    _ => Machine::<A, T>::opcode_unknown(machine, backend).await,
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

        #[cfg(not(target_os = "solana"))]
        impl<'de> serde::Deserialize<'de> for Opcode {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct OpcodeVisitor;

                impl<'de> serde::de::Visitor<'de> for OpcodeVisitor {
                    type Value = Opcode;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        write!(
                            formatter,
                            "one of opcodes like 'call', 'delegatecall' must be provided"
                        )
                    }

                    fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        for (pos, code) in OPNAMES.iter().enumerate() {
                            if *code == v {
                                return u8::try_from(pos).map(|c| Opcode(c)).map_err(
                                    |e| serde::de::Error::custom(e.to_string())
                                );
                            }
                        }

                        Err(serde::de::Error::custom(format!("Invalid opcode: {:?}", v)))
                    }
                }

                deserializer.deserialize_str(OpcodeVisitor)
            }
        }
    }
}

#[maybe_async]
impl<A, T> Machine<A, T>
where
    A: Allocator + Copy,
    T: EventListener,
{
    pub async fn execute_opcode(
        &mut self,
        backend: &mut impl Database,
        opcode: u8,
    ) -> Result<Action> {
        OpcodeTable::execute_opcode(self, backend, opcode).await
    }
}

opcode_table![
        0x00, STOP, Machine::<A, T>::opcode_stop;
        0x01, ADD, Machine::<A, T>::opcode_add;
        0x02, MUL, Machine::<A, T>::opcode_mul;
        0x03, SUB, Machine::<A, T>::opcode_sub;
        0x04, DIV, Machine::<A, T>::opcode_div;
        0x05, SDIV, Machine::<A, T>::opcode_sdiv;
        0x06, MOD, Machine::<A, T>::opcode_mod;
        0x07, SMOD, Machine::<A, T>::opcode_smod;
        0x08, ADDMOD, Machine::<A, T>::opcode_addmod;
        0x09, MULMOD, Machine::<A, T>::opcode_mulmod;
        0x0A, EXP, Machine::<A, T>::opcode_exp;
        0x0B, SIGNEXTEND, Machine::<A, T>::opcode_signextend;

        0x10, LT, Machine::<A, T>::opcode_lt;
        0x11, GT, Machine::<A, T>::opcode_gt;
        0x12, SLT, Machine::<A, T>::opcode_slt;
        0x13, SGT, Machine::<A, T>::opcode_sgt;
        0x14, EQ, Machine::<A, T>::opcode_eq;
        0x15, ISZERO, Machine::<A, T>::opcode_iszero;
        0x16, AND, Machine::<A, T>::opcode_and;
        0x17, OR, Machine::<A, T>::opcode_or;
        0x18, XOR, Machine::<A, T>::opcode_xor;
        0x19, NOT, Machine::<A, T>::opcode_not;
        0x1A, BYTE, Machine::<A, T>::opcode_byte;
        0x1B, SHL, Machine::<A, T>::opcode_shl;
        0x1C, SHR, Machine::<A, T>::opcode_shr;
        0x1D, SAR, Machine::<A, T>::opcode_sar;

        0x20, KECCAK256, Machine::<A, T>::opcode_sha3;

        0x30, ADDRESS, Machine::<A, T>::opcode_address;
        0x31, BALANCE, Machine::<A, T>::opcode_balance;
        0x32, ORIGIN, Machine::<A, T>::opcode_origin;
        0x33, CALLER, Machine::<A, T>::opcode_caller;
        0x34, CALLVALUE, Machine::<A, T>::opcode_callvalue;
        0x35, CALLDATALOAD, Machine::<A, T>::opcode_calldataload;
        0x36, CALLDATASIZE, Machine::<A, T>::opcode_calldatasize;
        0x37, CALLDATACOPY, Machine::<A, T>::opcode_calldatacopy;
        0x38, CODESIZE, Machine::<A, T>::opcode_codesize;
        0x39, CODECOPY, Machine::<A, T>::opcode_codecopy;
        0x3A, GASPRICE, Machine::<A, T>::opcode_gasprice;
        0x3B, EXTCODESIZE, Machine::<A, T>::opcode_extcodesize;
        0x3C, EXTCODECOPY, Machine::<A, T>::opcode_extcodecopy;
        0x3D, RETURNDATASIZE, Machine::<A, T>::opcode_returndatasize;
        0x3E, RETURNDATACOPY, Machine::<A, T>::opcode_returndatacopy;
        0x3F, EXTCODEHASH, Machine::<A, T>::opcode_extcodehash;
        0x40, BLOCKHASH, Machine::<A, T>::opcode_blockhash;
        0x41, COINBASE, Machine::<A, T>::opcode_coinbase;
        0x42, TIMESTAMP, Machine::<A, T>::opcode_timestamp;
        0x43, NUMBER, Machine::<A, T>::opcode_number;
        0x44, PREVRANDAO, Machine::<A, T>::opcode_difficulty;
        0x45, GASLIMIT, Machine::<A, T>::opcode_gaslimit;
        0x46, CHAINID, Machine::<A, T>::opcode_chainid;
        0x47, SELFBALANCE, Machine::<A, T>::opcode_selfbalance;
        0x48, BASEFEE, Machine::<A, T>::opcode_basefee;

        0x50, POP, Machine::<A, T>::opcode_pop;
        0x51, MLOAD, Machine::<A, T>::opcode_mload;
        0x52, MSTORE, Machine::<A, T>::opcode_mstore;
        0x53, MSTORE8, Machine::<A, T>::opcode_mstore8;
        0x54, SLOAD, Machine::<A, T>::opcode_sload;
        0x55, SSTORE, Machine::<A, T>::opcode_sstore;
        0x56, JUMP, Machine::<A, T>::opcode_jump;
        0x57, JUMPI, Machine::<A, T>::opcode_jumpi;
        0x58, PC, Machine::<A, T>::opcode_pc;
        0x59, MSIZE, Machine::<A, T>::opcode_msize;
        0x5A, GAS, Machine::<A, T>::opcode_gas;
        0x5B, JUMPDEST, Machine::<A, T>::opcode_jumpdest;

        0x5C, TLOAD, Machine::<A, T>::opcode_tload;
        0x5D, TSTORE, Machine::<A, T>::opcode_tstore;
        0x5E, MCOPY, Machine::<A, T>::opcode_mcopy;

        0x5F, PUSH0, Machine::<A, T>::opcode_push_0;
        0x60, PUSH1, Machine::<A, T>::opcode_push_1;
        0x61, PUSH2, Machine::<A, T>::opcode_push_2_31::<2>;
        0x62, PUSH3, Machine::<A, T>::opcode_push_2_31::<3>;
        0x63, PUSH4, Machine::<A, T>::opcode_push_2_31::<4>;
        0x64, PUSH5, Machine::<A, T>::opcode_push_2_31::<5>;
        0x65, PUSH6, Machine::<A, T>::opcode_push_2_31::<6>;
        0x66, PUSH7, Machine::<A, T>::opcode_push_2_31::<7>;
        0x67, PUSH8, Machine::<A, T>::opcode_push_2_31::<8>;
        0x68, PUSH9, Machine::<A, T>::opcode_push_2_31::<9>;
        0x69, PUSH10, Machine::<A, T>::opcode_push_2_31::<10>;
        0x6A, PUSH11, Machine::<A, T>::opcode_push_2_31::<11>;
        0x6B, PUSH12, Machine::<A, T>::opcode_push_2_31::<12>;
        0x6C, PUSH13, Machine::<A, T>::opcode_push_2_31::<13>;
        0x6D, PUSH14, Machine::<A, T>::opcode_push_2_31::<14>;
        0x6E, PUSH15, Machine::<A, T>::opcode_push_2_31::<15>;
        0x6F, PUSH16, Machine::<A, T>::opcode_push_2_31::<16>;
        0x70, PUSH17, Machine::<A, T>::opcode_push_2_31::<17>;
        0x71, PUSH18, Machine::<A, T>::opcode_push_2_31::<18>;
        0x72, PUSH19, Machine::<A, T>::opcode_push_2_31::<19>;
        0x73, PUSH20, Machine::<A, T>::opcode_push_2_31::<20>;
        0x74, PUSH21, Machine::<A, T>::opcode_push_2_31::<21>;
        0x75, PUSH22, Machine::<A, T>::opcode_push_2_31::<22>;
        0x76, PUSH23, Machine::<A, T>::opcode_push_2_31::<23>;
        0x77, PUSH24, Machine::<A, T>::opcode_push_2_31::<24>;
        0x78, PUSH25, Machine::<A, T>::opcode_push_2_31::<25>;
        0x79, PUSH26, Machine::<A, T>::opcode_push_2_31::<26>;
        0x7A, PUSH27, Machine::<A, T>::opcode_push_2_31::<27>;
        0x7B, PUSH28, Machine::<A, T>::opcode_push_2_31::<28>;
        0x7C, PUSH29, Machine::<A, T>::opcode_push_2_31::<29>;
        0x7D, PUSH30, Machine::<A, T>::opcode_push_2_31::<30>;
        0x7E, PUSH31, Machine::<A, T>::opcode_push_2_31::<31>;
        0x7F, PUSH32, Machine::<A, T>::opcode_push_32;

        0x80, DUP1, Machine::<A, T>::opcode_dup_1_16::<1>;
        0x81, DUP2, Machine::<A, T>::opcode_dup_1_16::<2>;
        0x82, DUP3, Machine::<A, T>::opcode_dup_1_16::<3>;
        0x83, DUP4, Machine::<A, T>::opcode_dup_1_16::<4>;
        0x84, DUP5, Machine::<A, T>::opcode_dup_1_16::<5>;
        0x85, DUP6, Machine::<A, T>::opcode_dup_1_16::<6>;
        0x86, DUP7, Machine::<A, T>::opcode_dup_1_16::<7>;
        0x87, DUP8, Machine::<A, T>::opcode_dup_1_16::<8>;
        0x88, DUP9, Machine::<A, T>::opcode_dup_1_16::<9>;
        0x89, DUP10, Machine::<A, T>::opcode_dup_1_16::<10>;
        0x8A, DUP11, Machine::<A, T>::opcode_dup_1_16::<11>;
        0x8B, DUP12, Machine::<A, T>::opcode_dup_1_16::<12>;
        0x8C, DUP13, Machine::<A, T>::opcode_dup_1_16::<13>;
        0x8D, DUP14, Machine::<A, T>::opcode_dup_1_16::<14>;
        0x8E, DUP15, Machine::<A, T>::opcode_dup_1_16::<15>;
        0x8F, DUP16, Machine::<A, T>::opcode_dup_1_16::<16>;

        0x90, SWAP1, Machine::<A, T>::opcode_swap_1_16::<1>;
        0x91, SWAP2, Machine::<A, T>::opcode_swap_1_16::<2>;
        0x92, SWAP3, Machine::<A, T>::opcode_swap_1_16::<3>;
        0x93, SWAP4, Machine::<A, T>::opcode_swap_1_16::<4>;
        0x94, SWAP5, Machine::<A, T>::opcode_swap_1_16::<5>;
        0x95, SWAP6, Machine::<A, T>::opcode_swap_1_16::<6>;
        0x96, SWAP7, Machine::<A, T>::opcode_swap_1_16::<7>;
        0x97, SWAP8, Machine::<A, T>::opcode_swap_1_16::<8>;
        0x98, SWAP9, Machine::<A, T>::opcode_swap_1_16::<9>;
        0x99, SWAP10, Machine::<A, T>::opcode_swap_1_16::<10>;
        0x9A, SWAP11, Machine::<A, T>::opcode_swap_1_16::<11>;
        0x9B, SWAP12, Machine::<A, T>::opcode_swap_1_16::<12>;
        0x9C, SWAP13, Machine::<A, T>::opcode_swap_1_16::<13>;
        0x9D, SWAP14, Machine::<A, T>::opcode_swap_1_16::<14>;
        0x9E, SWAP15, Machine::<A, T>::opcode_swap_1_16::<15>;
        0x9F, SWAP16, Machine::<A, T>::opcode_swap_1_16::<16>;

        0xA0, LOG0, Machine::<A, T>::opcode_log_0_4::<0>;
        0xA1, LOG1, Machine::<A, T>::opcode_log_0_4::<1>;
        0xA2, LOG2, Machine::<A, T>::opcode_log_0_4::<2>;
        0xA3, LOG3, Machine::<A, T>::opcode_log_0_4::<3>;
        0xA4, LOG4, Machine::<A, T>::opcode_log_0_4::<4>;

        0xF0, CREATE, Machine::<A, T>::opcode_create;
        0xF1, CALL, Machine::<A, T>::opcode_call;
        0xF2, CALLCODE, Machine::<A, T>::opcode_callcode;
        0xF3, RETURN, Machine::<A, T>::opcode_return;
        0xF4, DELEGATECALL, Machine::<A, T>::opcode_delegatecall;
        0xF5, CREATE2, Machine::<A, T>::opcode_create2;

        0xFA, STATICCALL, Machine::<A, T>::opcode_staticcall;

        0xFD, REVERT, Machine::<A, T>::opcode_revert;
        0xFE, INVALID, Machine::<A, T>::opcode_invalid;

        0xFF, SENDALL, Machine::<A, T>::opcode_sendall;
];
