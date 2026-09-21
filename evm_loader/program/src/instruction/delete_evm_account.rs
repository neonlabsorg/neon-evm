use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey, pubkey::Pubkey, system_program};

use crate::{
    account::{AccountRead, Balance, Container},
    debug::log_data,
    error::{Error, Result},
};

/// The only account authorized to delete arbitrary program owned accounts.
const AUTHORIZED_SIGNER: Pubkey = pubkey!("HdE1e3PAxurMZCi7Wh6zL5XrYtebQYUJCi54mNee2SZX");

fn report_balance<T: AccountRead>(account: Balance<T>) {
    if account.balance() == U256::ZERO {
        return;
    }

    let address = account.address();
    let address = address.as_bytes();

    let amount = account.balance().to_le_bytes();

    if let Some(pubkey) = account.solana_address() {
        log_data(&[b"delete_solana_balance", address, pubkey.as_ref(), &amount]);
    } else {
        log_data(&[b"delete_balance", address, &amount]);
    }
}

fn report_balances_in_container<T: AccountRead + Clone>(
    program_id: &Pubkey,
    container: Container<T>,
) {
    for key in container.keys().iter() {
        let account = container.account(&key.pubkey).unwrap();
        if let Ok(balance) = Balance::from_account(program_id, account) {
            report_balance(balance);
        }
    }
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    let (signer, targets) = accounts
        .split_first()
        .ok_or(Error::AccountMissing(AUTHORIZED_SIGNER))?;

    if *signer.key != AUTHORIZED_SIGNER {
        return Err(Error::AccountInvalidKey(*signer.key, AUTHORIZED_SIGNER));
    }

    if !signer.is_signer {
        return Err(Error::AccountNotSigner(*signer.key));
    }

    for account in targets {
        if account.owner != program_id {
            return Err(Error::AccountInvalidOwner(*account.key, *program_id));
        }

        if let Ok(balance) = Balance::from_account_info(program_id, account) {
            report_balance(balance);
        }

        if let Ok(container) = Container::from_account_info(program_id, account) {
            report_balances_in_container(program_id, container);
        }

        **signer.lamports.borrow_mut() += account.lamports();
        **account.lamports.borrow_mut() = 0;

        account.data.borrow_mut().fill(0);
        account.resize(0)?;
        account.assign(&system_program::ID);
    }

    Ok(())
}
