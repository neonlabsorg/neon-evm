//! # Neon EVM
//!
//! Neon EVM is an implementation of Ethereum Virtual Machine on Solana.
#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_const_for_fn,
    clippy::use_self,
    clippy::future_not_send,
    clippy::inline_always
)]
#![allow(
    missing_docs,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    unexpected_cfgs,
    clippy::new_without_default
)]

solana_program::declare_id!(crate::config::PROGRAM_ID);

mod allocator;
#[macro_use]
pub mod debug;
#[macro_use]
pub mod error;
pub mod account;
pub mod config;
#[cfg(target_os = "solana")]
pub mod entrypoint;
pub mod evm;
pub mod executor;
pub mod gasometer;
#[cfg(target_os = "solana")]
pub mod instruction;
#[macro_use]
pub mod types;
pub mod platform;
pub mod priority_gas_calculator;
#[cfg(target_os = "solana")]
pub mod transaction_process;

// Export current solana-sdk types for downstream users who may also be building with a different
// solana-sdk version
pub use solana_program;
