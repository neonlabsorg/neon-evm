use crate::{
    account::{MainTreasury, Operator},
    config::TREASURY_POOL_SEED,
    error::{Error, Result},
    executor::external_programs::{
        spl_token::{self, SPL_TOKEN_ID},
        system, Invokable,
    },
    platform::Solana,
    types::seeds::Seeds,
};
use pinocchio_token_interface::{native_mint, state::Transmutable};
use solana_program::{
    account_info::AccountInfo,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    pubkey::Pubkey,
};

struct Accounts<'r, 'a> {
    main_treasury: &'r AccountInfo<'a>,
    program_data: &'r AccountInfo<'a>,
    authority: &'r AccountInfo<'a>,
    payer: Operator<'a>,
    all: &'r [AccountInfo<'a>],
}

impl<'r, 'a> Accounts<'r, 'a> {
    pub fn from_slice(accounts: &'r [AccountInfo<'a>]) -> Result<Accounts<'r, 'a>> {
        Ok(Accounts {
            main_treasury: &accounts[0],
            program_data: &accounts[1],
            authority: &accounts[2],
            // token_program: accounts[3],
            // system_program: accounts[4],
            // native_mint: accounts[5],
            payer: unsafe { Operator::from_account_not_whitelisted(&accounts[6]) }?,
            all: accounts,
        })
    }
}

fn get_program_upgrade_authority(
    program_id: &Pubkey,
    program_data: &AccountInfo,
) -> Result<Pubkey> {
    let expected_key = bpf_loader_upgradeable::get_program_data_address(&program_id);

    if program_data.key != &expected_key {
        return Err(Error::AccountInvalidKey(*program_data.key, expected_key));
    }

    let program_data: UpgradeableLoaderState = bincode::deserialize(&program_data.data.borrow())?;
    let upgrade_authority: Pubkey = match program_data {
        UpgradeableLoaderState::ProgramData {
            slot: _,
            upgrade_authority_address,
        } => upgrade_authority_address.ok_or_else(|| Error::from("Not upgradeable program"))?,
        _ => return Err(Error::from("Not ProgramData")),
    };

    Ok(upgrade_authority)
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Create Main Treasury");

    let accounts = Accounts::from_slice(accounts)?;

    let (main_treasury, bump_seed) = MainTreasury::address(program_id);
    if accounts.main_treasury.key != &main_treasury {
        let error = Error::AccountInvalidKey(*accounts.main_treasury.key, main_treasury);
        return Err(error);
    }

    let authority = get_program_upgrade_authority(program_id, &accounts.program_data)?;
    if accounts.authority.key != &authority {
        let error = Error::AccountInvalidKey(*accounts.authority.key, authority);
        return Err(error);
    }
    if !accounts.authority.is_signer {
        return Err(Error::AccountNotSigner(*accounts.authority.key));
    }

    let mut solana = Solana::new(accounts.all, accounts.payer, None)?;

    let seeds = &[TREASURY_POOL_SEED.as_bytes(), &[bump_seed]];
    system::CreateAccount {
        account: main_treasury,
        seeds: Seeds::new(seeds),
        owner: SPL_TOKEN_ID,
        space: pinocchio_token_interface::state::account::Account::LEN,
    }
    .invoke(&mut solana)?;

    spl_token::InitializeAccount {
        account: main_treasury,
        mint: Pubkey::new_from_array(native_mint::ID),
        owner: authority,
    }
    .invoke(&mut solana)
}
