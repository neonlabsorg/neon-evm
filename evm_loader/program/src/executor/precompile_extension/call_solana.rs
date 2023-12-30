use crate::{
    //account::ACCOUNT_SEED_VERSION,
    types::Address,
    error::{Error, Result}, config::ACCOUNT_SEED_VERSION, evm::database::Database,
};
use solana_program::{instruction::{AccountMeta, Instruction}, pubkey::Pubkey};
use arrayref::array_ref;
use ethnum::U256;
use maybe_async::maybe_async;


// "91183f5a": "call(bytes32,(bytes32,uint8)[],bytes,uint64)"
// "cfd51d32": "createResource(bytes32,uint64,uint64,bytes32)"
// "cdf4e40f": "getExtResourceAddress(bytes32)"
// "154d4aa5": "getNeonAddress(address)"
// "59e4ad63": "getResourceAddress(bytes32)"
// "4a890f31": "getSolanaPDA(bytes32,bytes)"
// "09c5eabe": "execute(bytes)"


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
        [0x09, 0xc5, 0xea, 0xbe] => { // execute(bytes)
            let instruction: Instruction = bincode::deserialize(input)
                .map_err(|_| Error::OutOfBounds)?;

            #[cfg(not(target_os = "solana"))]
            log::info!("instruction: {:?}", instruction);

            let signer = context.caller;
            let (_signer_pubkey, bump_seed) = state.contract_pubkey(signer);

            let seeds = vec![
                vec![ACCOUNT_SEED_VERSION],
                signer.as_bytes().to_vec(),
                vec![bump_seed],
            ];

            // TODO: this instruction can create accounts inside, so we need to specify correct fee. How we can get it?
            state.queue_external_instruction(instruction, seeds, 0, false)?;

            Ok(vec![])
        }
        [0x91, 0x18, 0x3f, 0x5a] => {
            // Call solana program
            const FLAG_WRITABLE: u8 = 0x01;
            const FLAG_SIGNER: u8 = 0x02;

            let signer = context.caller;
            let (signer_pubkey, bump_seed) = state.contract_pubkey(signer);

            let seeds = vec![
                vec![ACCOUNT_SEED_VERSION],
                signer.as_bytes().to_vec(),
                vec![bump_seed],
            ];

            let program_id = read_pubkey(&input[0..])?;
            let accounts_offset = read_usize(&input[32..])?;
            let data_offset = read_usize(&input[64..])?;
            let _lamports = read_u64(&input[96..])?;

            let accounts_length = read_usize(&input[accounts_offset..])?;
            let mut accounts = Vec::with_capacity(accounts_length);
            for i in 0..accounts_length {
                let pubkey = read_pubkey(&input[accounts_offset+32+i*64..])?;
                let flags = read_u8(&input[accounts_offset+32+i*64+32..])?;
                if (flags & FLAG_SIGNER != 0 && pubkey != signer_pubkey) ||
                   (pubkey == state.operator())
                {
                    return Err(Error::InvalidAccountForCall(pubkey));
                }
                accounts.push(AccountMeta {
                    pubkey,
                    is_writable: flags & FLAG_WRITABLE != 0,
                    is_signer: flags & FLAG_SIGNER != 0,
                })
            }

            let data_length = read_usize(&input[data_offset..])?;
            let data = &input[data_offset+32..data_offset+32+data_length];
        
            let instruction = Instruction::new_with_bytes(program_id, data, accounts);

            // TODO: this instruction can create accounts inside, so we need to specify correct fee. How we can get it?
            state.queue_external_instruction(instruction, seeds, 0, false)?;

            Ok(vec![])
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

        // "cdf4e40f": "getExtResourceAddress(bytes32)"
        [0xcd, 0xf4, 0xe4, 0x0f] => {
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
            let mut seeds = Vec::with_capacity((length+31)/32);
            for i in 0..length/32 {
                seeds.push(&input[offset+32+i*32..offset+32+(i+1)*32]);
            }
            if length%32 != 0 {
                seeds.push(&input[offset+32+length-length%32..offset+32+length]);
            }
            let (sol_address, _) = Pubkey::find_program_address(&seeds, &program_id);
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
        
            super::create_account(state, &account, space, &owner, seeds)?;
            Ok(sol_address.to_bytes().to_vec())
        }

        _ => Err(Error::UnknownPrecompileMethodSelector(*address, selector)),
    }
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

#[allow(warnings)]
#[cfg(test)]
mod tests {
    use solana_program_test::{processor, ProgramTest};
    use solana_program::pubkey::Pubkey;
    use solana_program::account_info::AccountInfo;
    use solana_program::entrypoint::ProgramResult;
    use solana_sdk::account::Account;
    use solana_program::rent::Rent;
    use std::str::FromStr;
    use hex::FromHex;
    // use crate::account::Packable;

    // fn process_instruction<'a,'b>(
    //     program_id: &Pubkey,
    //     accounts: &[AccountInfo],
    //     instruction_data: &[u8],
    // ) -> ProgramResult {
    //     unsafe {
    //         crate::entrypoint::process_instruction(program_id, accounts, instruction_data)
    //     }
    // }

    #[test]
    fn decode_arguments() {
        let program_id = Pubkey::from_str("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io").unwrap();
        let mut program_test = ProgramTest::new(
            "evm_loader",
            program_id,
            None
 //           processor!(process_instruction)
        );


        let address = crate::types::Address::from_hex("0x102030405060708090a0102030405060708090a0").unwrap();
        let (solana_address, bump_seed) = address.find_solana_address(&program_id);
        //program_test.add_program("evm_loader", program_id, None);
        // let contract = crate::account::ether_account::Data {
        //     address,
        //     bump_seed,
        //     trx_count: 0,
        //     balance: 0u64.into(),
        //     generation: 0,
        //     code_size: 0,
        //     rw_blocked: false
        // };
        // let bytecode = super::BYTE_CODE.to_string().replace("\n", "");

        // let contract_code = hex::decode(bytecode).unwrap();
        // let mut data = vec![0u8; crate::account::ether_account::Data::SIZE + 1 + 32*32 + contract_code.len()];
        // data[0] = crate::account::ether_account::Data::TAG;
        // contract.pack(&mut data[1..1+crate::account::ether_account::Data::SIZE]);
        // data[crate::account::ether_account::Data::SIZE + 1 + 32*32..].clone_from_slice(&contract_code);

        // //println!("{}", hex::encode(data.clone()));
        // program_test.add_account(
        //     solana_address,
        //     Account {
        //         owner: program_id,
        //         lamports: Rent::default().minimum_balance(data.len()),
        //         data,
        //         executable: false,
        //         ..Default::default()
        //     }
        // );

    }
}
