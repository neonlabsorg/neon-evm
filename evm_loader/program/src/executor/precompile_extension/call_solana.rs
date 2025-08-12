use crate::{
    account::{pda, InterruptedState},
    error::{Error, Result},
    executor::external_programs::{system::CreateAccount, SystemProgram},
    platform::{KeysIndex, FAKE_OPERATOR},
    types::{seeds::Seeds, Address},
};

use allocator_api2::alloc::Allocator;
use arrayref::array_ref;
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use solana_system_interface::instruction as system_instruction;

use super::PrecompileDatabase;

// "cfd51d32": "createResource(bytes32,uint64,uint64,bytes32)"
// "154d4aa5": "getNeonAddress(address)"
// "59e4ad63": "getResourceAddress(bytes32)"
// "4a890f31": "getSolanaPDA(bytes32,bytes)"
// "cd2d1a3a": "getExtAuthority(bytes32)"
// "30aa81c6": "getPayer()",

// "09c5eabe": "execute(bytes)"
// "c549a7af": "execute(uint64,bytes)",
// "32607450": "executeWithSeed(uint64,bytes32,bytes)",
// "260e40bc": "executeWithSeed(bytes32,bytes)",
// "aeed7f1e": "execute(uint64,(bytes32,(bytes32,bool,bool)[],bytes))",
// "a5754fda": "execute((bytes32,(bytes32,bool,bool)[],bytes))",
// "8ea46ee8": "executeWithSeed(bytes32,(bytes32,(bytes32,bool,bool)[],bytes))",
// "add378af": "executeWithSeed(uint64,bytes32,(bytes32,(bytes32,bool,bool)[],bytes))",
// "cff5c1a5": "getReturnData()",

#[maybe_async]
#[allow(clippy::too_many_lines)]
pub async fn call_solana(
    state: &mut impl PrecompileDatabase,
    address: &Address,
    input: &[u8],
    context: &crate::evm::Context,
    is_static: bool,
) -> Result<Vec<u8>> {
    if context.value != 0 {
        return Err(Error::Custom("CallSolana: value != 0".to_string()));
    }

    if &context.contract != address {
        return Err(Error::Custom(
            "CallSolana: callcode or delegatecall is not allowed".to_string(),
        ));
    }

    let program_id = state.program_id();

    let (selector, input) = input.split_at(4);
    let selector: [u8; 4] = selector.try_into()?;

    debug_print!("Call arguments: {}", hex::encode(input));

    match selector {
        // "09c5eabe": "execute(bytes)",
        [0x09, 0xc5, 0xea, 0xbe] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let offset = read_usize(input)?;
            let instruction: Instruction =
                bincode::deserialize(&input[32 + offset..]).map_err(|_| Error::OutOfBounds)?;

            let signer = context.caller;
            let (_, bump_seed) = state.keys().contract_bump(signer);
            let seeds: &[&[u8]] = pda::contract_seeds!(signer, bump_seed);

            execute_instruction(state, context, instruction, seeds, None).await
        }
        // "c549a7af": "execute(uint64,bytes)",
        [0xc5, 0x49, 0xa7, 0xaf] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let required_lamports = read_u64(&input[0..])?;
            let offset = read_usize(&input[32..])?;
            let instruction: Instruction =
                bincode::deserialize(&input[offset + 32..]).map_err(|_| Error::OutOfBounds)?;

            let signer = context.caller;
            let (_, bump_seed) = state.keys().contract_bump(signer);
            let seeds: &[&[u8]] = pda::contract_seeds!(signer, bump_seed);

            execute_instruction(state, context, instruction, seeds, Some(required_lamports)).await
        }

        // "260e40bc": "executeWithSeed(bytes32,bytes)",
        [0x26, 0x0e, 0x40, 0xbc] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let salt = read_salt(input)?;
            let offset = read_usize(&input[32..])?;
            let instruction: Instruction = bincode::deserialize(&input[offset + 32..])?;

            let signer = context.caller;
            let (_, bump_seed) = pda::contract_auth(program_id, &signer, salt);
            let seeds: &[&[u8]] = pda::contract_auth_seeds!(signer, salt, bump_seed);

            execute_instruction(state, context, instruction, seeds, None).await
        }

        // "32607450": "executeWithSeed(uint64,bytes32,bytes)",
        [0x32, 0x60, 0x74, 0x50] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let required_lamports = read_u64(&input[0..])?;
            let salt = read_salt(&input[32..])?;
            let offset = read_usize(&input[64..])?;
            let instruction: Instruction = bincode::deserialize(&input[offset + 32..])?;

            let signer = context.caller;
            let (_, bump_seed) = pda::contract_auth(program_id, &signer, salt);
            let seeds: &[&[u8]] = pda::contract_auth_seeds!(signer, salt, bump_seed);

            execute_instruction(state, context, instruction, seeds, Some(required_lamports)).await
        }

        // "aeed7f1e": "execute(uint64,(bytes32,(bytes32,bool,bool)[],bytes))",
        [0xae, 0xed, 0x7f, 0x1e] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let required_lamports = read_u64(&input[0..])?;
            let instruction_offset = read_usize(&input[32..])?;
            let instruction = read_instruction(&input[instruction_offset..])?;

            let signer = context.caller;
            let (_, bump_seed) = state.keys().contract_bump(signer);
            let seeds: &[&[u8]] = pda::contract_seeds!(signer, bump_seed);

            execute_instruction(state, context, instruction, seeds, Some(required_lamports)).await
        }

        // "a5754fda": "execute((bytes32,(bytes32,bool,bool)[],bytes))",
        [0xa5, 0x75, 0x4f, 0xda] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let instruction_offset = read_usize(&input[0..])?;
            let instruction = read_instruction(&input[instruction_offset..])?;

            let signer = context.caller;
            let (_, bump_seed) = state.keys().contract_bump(signer);
            let seeds: &[&[u8]] = pda::contract_seeds!(signer, bump_seed);

            execute_instruction(state, context, instruction, seeds, None).await
        }

        // "8ea46ee8": "executeWithSeed(bytes32,(bytes32,(bytes32,bool,bool)[],bytes))",
        [0x8e, 0xa4, 0x6e, 0xe8] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let salt = read_salt(&input[0..])?;
            let instruction_offset = read_usize(&input[32..])?;
            let instruction = read_instruction(&input[instruction_offset..])?;

            let signer = context.caller;
            let (_, bump_seed) = pda::contract_auth(program_id, &signer, salt);
            let seeds: &[&[u8]] = pda::contract_auth_seeds!(signer, salt, bump_seed);

            execute_instruction(state, context, instruction, seeds, None).await
        }

        // "add378af": "executeWithSeed(uint64,bytes32,(bytes32,(bytes32,bool,bool)[],bytes))",
        [0xad, 0xd3, 0x78, 0xaf] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let required_lamports = read_u64(&input[0..])?;
            let salt = read_salt(&input[32..])?;
            let instruction_offset = read_usize(&input[64..])?;
            let instruction = read_instruction(&input[instruction_offset..])?;

            let signer = context.caller;
            let (_, bump_seed) = pda::contract_auth(program_id, &signer, salt);
            let seeds: &[&[u8]] = pda::contract_auth_seeds!(signer, salt, bump_seed);

            execute_instruction(state, context, instruction, seeds, Some(required_lamports)).await
        }

        // "154d4aa5": "getNeonAddress(address)"
        [0x15, 0x4d, 0x4a, 0xa5] => {
            let neon_addess = Address::from(*array_ref![input, 12, 20]);
            let (sol_address, _) = state.keys().contract_bump(neon_addess);
            Ok(sol_address.to_bytes().to_vec())
        }

        // "59e4ad63": "getResourceAddress(bytes32)"
        [0x59, 0xe4, 0xad, 0x63] => {
            let salt = read_salt(input)?;
            let (sol_address, _) = pda::contract_data(program_id, &context.caller, salt);
            Ok(sol_address.to_bytes().to_vec())
        }

        // "cd2d1a3a": "getExtAuthority(bytes32)"
        [0xcd, 0x2d, 0x1a, 0x3a] => {
            let salt = read_salt(input)?;
            let (sol_address, _) = pda::contract_auth(program_id, &context.caller, salt);
            Ok(sol_address.to_bytes().to_vec())
        }

        // "4a890f31": "getSolanaPDA(bytes32,bytes)"
        [0x4a, 0x89, 0x0f, 0x31] => {
            let program_id = read_pubkey(&input[0..])?;
            let offset = read_usize(&input[32..])?;
            let length = read_usize(&input[offset..])?;
            let mut seeds = Vec::with_capacity((length + 31) / 32);
            for i in 0..length / 32 {
                seeds.push(&input[offset + 32 + i * 32..offset + 32 + (i + 1) * 32]);
            }
            if length % 32 != 0 {
                seeds.push(&input[offset + 32 + length - length % 32..offset + 32 + length]);
            }
            let (sol_address, _) = Pubkey::find_program_address(&seeds, &program_id);
            Ok(sol_address.to_bytes().to_vec())
        }

        // "30aa81c6": "getPayer()"
        [0x30, 0xaa, 0x81, 0xc6] => {
            let (sol_address, _) = pda::contract_payer(program_id, &context.caller);
            Ok(sol_address.to_bytes().to_vec())
        }

        // "cfd51d32": "createResource(bytes32,uint64,uint64,bytes32)"
        [0xcf, 0xd5, 0x1d, 0x32] => {
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let salt = read_salt(&input[0..])?;
            let space = read_usize(&input[32..])?;
            let _lamports = read_u64(&input[64..])?;
            let owner = read_pubkey(&input[96..])?;

            create_resource(state, context, salt, space, owner).await
        }

        // "cff5c1a5": "getReturnData()",
        [0xcf, 0xf5, 0xc1, 0xa5] => {
            let return_value = match state.return_data() {
                Some((program, data)) => {
                    let data_len = (data.len() + 31) & (!31);
                    let mut result = vec![0_u8; 32 + 32 + 32 + data_len];

                    result[0..32].copy_from_slice(&program.to_bytes());
                    result[63] = 0x40; // offset - 64 bytes

                    let length = U256::new(data.len() as u128);
                    result[64..96].copy_from_slice(&length.to_be_bytes());
                    result[96..96 + data.len()].copy_from_slice(&data);

                    result
                }
                None => {
                    vec![
                        // program_id
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        // offset of data
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40,
                        // length of data
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    ]
                }
            };

            Ok(return_value)
        }

        _ => Err(Error::UnknownPrecompileMethodSelector(*address, selector)),
    }
}

#[maybe_async]
pub async fn create_resource(
    state: &mut impl PrecompileDatabase,
    context: &crate::evm::Context,
    salt: &[u8; 32],
    space: usize,
    owner: Pubkey,
) -> Result<Vec<u8>> {
    let program_id = state.program_id();
    let signer = context.caller;

    let (account, bump_seed) = pda::contract_data(program_id, &signer, salt);
    let seeds: &[&[u8]] = pda::contract_data_seeds!(signer, salt, bump_seed);

    let create_account = SystemProgram::CreateAccount(CreateAccount {
        account,
        seeds: Seeds::new(seeds),
        owner,
        space,
    });
    state.queue_invoke(create_account).await?;

    Ok(account.to_bytes().to_vec())
}

#[maybe_async]
pub async fn execute_from_interrupt<A: Allocator>(
    state: &mut impl PrecompileDatabase,
    context: &crate::evm::Context,
    interrupt: &InterruptedState<A>,
) -> Result<Vec<u8>> {
    let instruction = interrupt.instruction();
    let signer_seeds = interrupt.seeds();
    let required_lamports = interrupt.lamports();

    execute_instruction(
        state,
        context,
        instruction,
        &signer_seeds,
        required_lamports,
    )
    .await
}

#[maybe_async]
pub async fn execute_instruction(
    state: &mut impl PrecompileDatabase,
    context: &crate::evm::Context,
    instruction: Instruction,
    signer_seeds: &[&[u8]],
    required_lamports: Option<u64>,
) -> Result<Vec<u8>> {
    debug_print!("instruction: {:?}", instruction);

    if state.is_iterative_mode() {
        let seeds: Vec<Vec<u8>> = signer_seeds.iter().map(|s| s.to_vec()).collect();
        let interrupt = std::boxed::Box::new((instruction, seeds, required_lamports));
        return Err(Error::InterruptedCall(interrupt));
    }

    let called_program = instruction.program_id;

    if &called_program == state.program_id() {
        return Err(Error::RecursiveCall);
    }

    for meta in &instruction.accounts {
        if meta.pubkey == FAKE_OPERATOR
            || &meta.pubkey == state.operator()
            || &meta.pubkey == state.program_id()
        {
            return Err(Error::InvalidAccountForCall(meta.pubkey));
        }
    }

    let (payer_pubkey, payer_bump_seed) = pda::contract_payer(state.program_id(), &context.caller);
    let required_payer = instruction
        .accounts
        .iter()
        .any(|meta| meta.pubkey == payer_pubkey);

    if required_payer {
        let payer_seeds: &[&[u8]] = pda::contract_payer_seeds!(&context.caller, payer_bump_seed);

        // transfer lamports to the payer account
        let lamports = match required_lamports {
            Some(lamports) => lamports,
            None => state.raw_lamports(state.operator()).await?,
        };

        let transfer = system_instruction::transfer(&FAKE_OPERATOR, &payer_pubkey, lamports);
        state.invoke(transfer, &[]).await?;

        // execute the instruction
        let seeds: &[&[&[u8]]] = &[signer_seeds, payer_seeds];
        state.invoke(instruction, seeds).await?;

        // transfer lamports from the payer account
        let lamports = state.raw_lamports(&payer_pubkey).await?;
        if lamports > 0 {
            let transfer = system_instruction::transfer(&payer_pubkey, &FAKE_OPERATOR, lamports);
            state.invoke(transfer, &[payer_seeds]).await?;
        }
    } else {
        state.invoke(instruction, &[signer_seeds]).await?;
    }

    let (_, return_data) = state
        .return_data()
        .filter(|(program, _)| program == &called_program)
        .unwrap_or_default();

    Ok(to_solidity_bytes(&return_data))
}

#[inline]
fn read_instruction(input: &[u8]) -> Result<Instruction> {
    let program_id = read_pubkey(&input[0..])?;
    let accounts_offset = read_usize(&input[32..])?;

    let data_offset = read_usize(&input[64..])?;
    let data_length = read_usize(&input[data_offset..])?;

    let accounts_length = read_usize(&input[accounts_offset..])?;
    let mut accounts = Vec::with_capacity(accounts_length);
    for i in 0..accounts_length {
        let acc_offset = accounts_offset + 32 + i * 96;
        let pubkey = read_pubkey(&input[acc_offset..])?;
        let is_signer = read_u8(&input[acc_offset + 32..])? != 0;
        let is_writable = read_u8(&input[acc_offset + 64..])? != 0;
        accounts.push(AccountMeta {
            pubkey,
            is_signer,
            is_writable,
        });
    }

    Ok(Instruction {
        program_id,
        accounts,
        data: input[data_offset + 32..data_offset + 32 + data_length].to_vec(),
    })
}

#[inline]
fn read_u8(input: &[u8]) -> Result<u8> {
    U256::from_be_bytes(*arrayref::array_ref![input, 0, 32])
        .try_into()
        .map_err(Into::into)
}

#[inline]
fn read_u64(input: &[u8]) -> Result<u64> {
    U256::from_be_bytes(*arrayref::array_ref![input, 0, 32])
        .try_into()
        .map_err(Into::into)
}

#[inline]
fn read_usize(input: &[u8]) -> Result<usize> {
    U256::from_be_bytes(*arrayref::array_ref![input, 0, 32])
        .try_into()
        .map_err(Into::into)
}

#[inline]
fn read_pubkey(input: &[u8]) -> Result<Pubkey> {
    if input.len() < 32 {
        return Err(Error::OutOfBounds);
    }
    Ok(Pubkey::new_from_array(*arrayref::array_ref![input, 0, 32]))
}

#[inline]
fn read_salt(input: &[u8]) -> Result<&[u8; 32]> {
    if input.len() < 32 {
        return Err(Error::OutOfBounds);
    }
    Ok(arrayref::array_ref![input, 0, 32])
}

fn to_solidity_bytes(b: &[u8]) -> Vec<u8> {
    // Bytes encoding
    // 32 bytes - offset
    // 32 bytes - length
    // length + padding bytes - data

    let data_len = (b.len() + 31) & (!31);
    let mut result = vec![0_u8; 32 + 32 + data_len];

    result[31] = 0x20; // offset - 32 bytes

    let length = U256::new(b.len() as u128);
    result[32..64].copy_from_slice(&length.to_be_bytes());
    result[64..64 + b.len()].copy_from_slice(b);

    result
}
