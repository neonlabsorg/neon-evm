use crate::{
    config::ACCOUNT_SEED_VERSION,
    error::{Error, Result},
    evm::database::Database,
    //account::ACCOUNT_SEED_VERSION,
    types::Address,
};
use arrayref::array_ref;
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

// "cfd51d32": "createResource(bytes32,uint64,uint64,bytes32)"
// "154d4aa5": "getNeonAddress(address)"
// "59e4ad63": "getResourceAddress(bytes32)"
// "4a890f31": "getSolanaPDA(bytes32,bytes)"
// "cd2d1a3a": "getExtAuthority(bytes32)"
// "30aa81c6": "getPayer()",

// "c549a7af": "execute(uint64,bytes)",
// "32607450": "executeWithSeed(uint64,bytes32,bytes)",
// "aeed7f1e": "execute(uint64,(bytes32,(bytes32,bool,bool)[],bytes))",
// "add378af": "executeWithSeed(uint64,bytes32,(bytes32,(bytes32,bool,bool)[],bytes))",
// "cff5c1a5": "getReturnData()",

#[maybe_async]
#[allow(clippy::too_many_lines)]
pub async fn call_solana<State: Database>(
    state: &mut State,
    address: &Address,
    input: &[u8],
    context: &crate::evm::Context,
    _is_static: bool,
) -> Result<Vec<u8>> {
    if context.value != 0 {
        return Err(Error::Custom("CallSolana: value != 0".to_string()));
    }

    if &context.contract != address {
        return Err(Error::Custom(
            "CallSolana: callcode or delegatecall is not allowed".to_string(),
        ));
    }

    let (selector, input) = input.split_at(4);
    let selector: [u8; 4] = selector.try_into()?;

    #[cfg(not(target_os = "solana"))]
    log::info!("Call arguments: {}", hex::encode(input));

    match selector {
        // "c549a7af": "execute(uint64,bytes)",
        [0xc5, 0x49, 0xa7, 0xaf] => {
            let required_lamports = read_u64(&input[0..])?;
            let offset = read_usize(&input[32..])?;
            let instruction: Instruction =
                bincode::deserialize(&input[offset + 32..]).map_err(|_| Error::OutOfBounds)?;

            let signer = context.caller;
            let (_signer_pubkey, bump_seed) = state.contract_pubkey(signer);

            let signer_seeds = vec![
                vec![ACCOUNT_SEED_VERSION],
                signer.as_bytes().to_vec(),
                vec![bump_seed],
            ];

            execute_external_instruction(
                state,
                context,
                instruction,
                signer_seeds,
                required_lamports,
            )
            .await
        }

        // "32607450": "executeWithSeed(uint64,bytes32,bytes)",
        [0x32, 0x60, 0x74, 0x50] => {
            let required_lamports = read_u64(&input[0..])?;
            let salt = read_salt(&input[32..])?;
            let offset = read_usize(&input[64..])?;
            let instruction: Instruction =
                bincode::deserialize(&input[offset + 32..]).map_err(|_| Error::OutOfBounds)?;

            let seeds: &[&[u8]] = &[
                &[ACCOUNT_SEED_VERSION],
                b"AUTH",
                context.caller.as_bytes(),
                salt,
            ];
            let (_, signer_seed) = Pubkey::find_program_address(seeds, state.program_id());
            let seeds = vec![
                vec![ACCOUNT_SEED_VERSION],
                b"AUTH".to_vec(),
                context.caller.as_bytes().to_vec(),
                salt.to_vec(),
                vec![signer_seed],
            ];

            execute_external_instruction(state, context, instruction, seeds, required_lamports)
                .await
        }

        // "aeed7f1e": "execute(uint64,(bytes32,(bytes32,bool,bool)[],bytes))",
        [0xae, 0xed, 0x7f, 0x1e] => {
            let required_lamports = read_u64(&input[0..])?;
            let instruction_offset = read_usize(&input[32..])?;
            let instruction = read_instruction(&input[instruction_offset..])?;

            let signer = context.caller;
            let (_signer_pubkey, bump_seed) = state.contract_pubkey(signer);

            let signer_seeds = vec![
                vec![ACCOUNT_SEED_VERSION],
                signer.as_bytes().to_vec(),
                vec![bump_seed],
            ];

            execute_external_instruction(
                state,
                context,
                instruction,
                signer_seeds,
                required_lamports,
            )
            .await
        }

        // "add378af": "executeWithSeed(uint64,bytes32,(bytes32,(bytes32,bool,bool)[],bytes))",
        [0xad, 0xd3, 0x78, 0xaf] => {
            let required_lamports = read_u64(&input[0..])?;
            let salt = read_salt(&input[32..])?;
            let instruction_offset = read_usize(&input[64..])?;
            let instruction = read_instruction(&input[instruction_offset..])?;

            let seeds: &[&[u8]] = &[
                &[ACCOUNT_SEED_VERSION],
                b"AUTH",
                context.caller.as_bytes(),
                salt,
            ];
            let (_, signer_seed) = Pubkey::find_program_address(seeds, state.program_id());
            let seeds = vec![
                vec![ACCOUNT_SEED_VERSION],
                b"AUTH".to_vec(),
                context.caller.as_bytes().to_vec(),
                salt.to_vec(),
                vec![signer_seed],
            ];

            execute_external_instruction(state, context, instruction, seeds, required_lamports)
                .await
        }

        // "154d4aa5": "getNeonAddress(address)"
        [0x15, 0x4d, 0x4a, 0xa5] => {
            let neon_addess = Address::from(*array_ref![input, 12, 20]);
            let sol_address = state.contract_pubkey(neon_addess).0;
            Ok(sol_address.to_bytes().to_vec())
        }

        // "59e4ad63": "getResourceAddress(bytes32)"
        [0x59, 0xe4, 0xad, 0x63] => {
            let salt = read_salt(input)?;
            let seeds: &[&[u8]] = &[
                &[ACCOUNT_SEED_VERSION],
                b"ContractData",
                context.caller.as_bytes(),
                salt,
            ];
            let (sol_address, _) = Pubkey::find_program_address(seeds, state.program_id());
            Ok(sol_address.to_bytes().to_vec())
        }

        // "cd2d1a3a": "getExtAuthority(bytes32)"
        [0xcd, 0x2d, 0x1a, 0x3a] => {
            let salt = read_salt(input)?;
            let seeds: &[&[u8]] = &[
                &[ACCOUNT_SEED_VERSION],
                b"AUTH",
                context.caller.as_bytes(),
                salt,
            ];
            let (sol_address, _) = Pubkey::find_program_address(seeds, state.program_id());
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
            let seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], b"PAYER", context.caller.as_bytes()];
            let (sol_address, _bump_seed) = Pubkey::find_program_address(seeds, state.program_id());

            Ok(sol_address.to_bytes().to_vec())
        }

        // "cfd51d32": "createResource(bytes32,uint64,uint64,bytes32)"
        [0xcf, 0xd5, 0x1d, 0x32] => {
            let salt = read_salt(&input[0..])?;
            let space = read_usize(&input[32..])?;
            let _lamports = read_u64(&input[64..])?;
            let owner = read_pubkey(&input[96..])?;

            let (sol_address, bump_seed) = Pubkey::find_program_address(
                &[
                    &[ACCOUNT_SEED_VERSION],
                    b"ContractData",
                    context.caller.as_bytes(),
                    salt,
                ],
                state.program_id(),
            );
            let account = state.external_account(sol_address).await?;
            let seeds: Vec<Vec<u8>> = vec![
                vec![ACCOUNT_SEED_VERSION],
                b"ContractData".to_vec(),
                context.caller.as_bytes().to_vec(),
                salt.to_vec(),
                vec![bump_seed],
            ];

            super::create_account(state, &account, space, &owner, seeds).await?;
            Ok(sol_address.to_bytes().to_vec())
        }

        // "cff5c1a5": "getReturnData()",
        [0xcf, 0xf5, 0xc1, 0xa5] => {
            let return_value = match solana_program::program::get_return_data() {
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
async fn execute_external_instruction<State: Database>(
    state: &mut State,
    context: &crate::evm::Context,
    instruction: Instruction,
    signer_seeds: Vec<Vec<u8>>,
    required_lamports: u64,
) -> Result<Vec<u8>> {
    #[cfg(not(target_os = "solana"))]
    log::info!("instruction: {:?}", instruction);

    let called_program = instruction.program_id;
    solana_program::program::set_return_data(&[]);

    for meta in &instruction.accounts {
        if meta.pubkey == state.operator() {
            return Err(Error::InvalidAccountForCall(state.operator()));
        }
    }

    let payer_seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], b"PAYER", context.caller.as_bytes()];
    let (payer_pubkey, payer_bump_seed) =
        Pubkey::find_program_address(payer_seeds, state.program_id());
    let required_payer = instruction
        .accounts
        .iter()
        .any(|meta| meta.pubkey == payer_pubkey);

    if required_payer {
        let payer_seeds = vec![
            vec![ACCOUNT_SEED_VERSION],
            b"PAYER".to_vec(),
            context.caller.as_bytes().to_vec(),
            vec![payer_bump_seed],
        ];

        let payer = state.external_account(payer_pubkey).await?;
        if payer.lamports < required_lamports {
            let transfer_instruction = solana_program::system_instruction::transfer(
                &state.operator(),
                &payer_pubkey,
                required_lamports - payer.lamports,
            );
            state.queue_external_instruction(transfer_instruction, vec![], 0, false).await?;
        }

        state.queue_external_instruction(
            instruction,
            vec![signer_seeds, payer_seeds.clone()],
            required_lamports,
            false,
        ).await?;

        let payer = state.external_account(payer_pubkey).await?;
        if payer.lamports > 0 {
            let transfer_instruction = solana_program::system_instruction::transfer(
                &payer_pubkey,
                &state.operator(),
                payer.lamports,
            );
            state.queue_external_instruction(transfer_instruction, vec![payer_seeds], 0, false).await?;
        }
    } else {
        state.queue_external_instruction(
            instruction,
            vec![signer_seeds],
            required_lamports,
            false,
        ).await?;
    }

    let return_data = solana_program::program::get_return_data()
        .and_then(|(program, data)| {
            if program == called_program {
                Some(data)
            } else {
                None
            }
        })
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
