mod legacy_ether;
mod legacy_storage_cell;

pub use legacy_ether::LegacyEtherData;
pub use legacy_storage_cell::LegacyStorageData;
use solana_program::{account_info::AccountInfo, rent::Rent, sysvar::Sysvar};

use super::{AccountsDB, ContractAccount, Operator, TAG_STORAGE_CELL};
use crate::{
    account::{BalanceAccount, StorageCell},
    account_storage::KeysCache,
    error::Result,
};

const _TAG_STATE_DEPRECATED: u8 = 22;
const _TAG_STATE_FINALIZED_DEPRECATED: u8 = 31;
const _TAG_HOLDER_DEPRECATED: u8 = 51;
pub const TAG_ACCOUNT_CONTRACT_DEPRECATED: u8 = 12;
pub const TAG_STORAGE_CELL_DEPRECATED: u8 = 42;

fn reduce_account_size(
    account: &AccountInfo,
    required_len: usize,
    operator: &Operator,
) -> Result<()> {
    assert!(account.data_len() > required_len);

    account.realloc(required_len, false)?;

    // Return excessive lamports to the operator
    let rent = Rent::get()?;
    let minimum_balance = rent.minimum_balance(account.data_len());
    if account.lamports() > minimum_balance {
        let value = account.lamports() - minimum_balance;

        **account.lamports.borrow_mut() -= value;
        **operator.lamports.borrow_mut() += value;
    }

    Ok(())
}

fn update_ether_account(
    legacy_data: &LegacyEtherData,
    db: &AccountsDB,
    keys: &KeysCache,
) -> Result<()> {
    let pubkey = keys.contract(&crate::ID, legacy_data.address);
    let account = db.get(&pubkey);

    if (legacy_data.generation > 0) || (legacy_data.code_size > 0) {
        // This is contract account. Convert it to new format
        super::validate_tag(&crate::ID, account, TAG_ACCOUNT_CONTRACT_DEPRECATED)?;

        // Read existing data
        let storage = legacy_data.read_storage(account)?;
        let code = legacy_data.read_code(account)?;

        // Make account smaller
        let required_len = ContractAccount::required_account_size(&code);
        reduce_account_size(account, required_len, db.operator())?;

        // Fill it with new data
        account.try_borrow_mut_data()?.fill(0);

        let mut contract = ContractAccount::init(
            legacy_data.address,
            crate::config::DEFAULT_CHAIN_ID,
            legacy_data.generation,
            &code,
            db,
            Some(keys),
        )?;
        contract.set_storage_multiple_values(0, &storage);

        super::set_block(&crate::ID, account, legacy_data.rw_blocked)?;
    } else {
        // This is not contract. Just kill it.
        // Transfer all lamports to operator
        unsafe {
            super::delete(account, db.operator());
        }
    }

    if (legacy_data.balance > 0) || (legacy_data.trx_count > 0) {
        // Has balance data. Create a new account
        let mut balance = BalanceAccount::create(
            legacy_data.address,
            crate::config::DEFAULT_CHAIN_ID,
            db,
            Some(keys),
        )?;
        balance.mint(legacy_data.balance)?;
        balance.increment_nonce_by(legacy_data.trx_count)?;

        super::set_block(&crate::ID, db.get(balance.pubkey()), legacy_data.rw_blocked)?;
    }

    Ok(())
}

fn update_storage_account(
    legacy_data: &LegacyStorageData,
    db: &AccountsDB,
    keys: &KeysCache,
) -> Result<()> {
    let cell_pubkey = keys.storage_cell(&crate::ID, legacy_data.address, legacy_data.index);
    let cell_account = db.get(&cell_pubkey).clone();

    let contract_pubkey = keys.contract(&crate::ID, legacy_data.address);
    let contract_account = db.get(&contract_pubkey).clone();
    let contract = ContractAccount::from_account(&crate::ID, contract_account)?;

    if contract.generation() != legacy_data.generation {
        // Cell is out of date. Kill it.
        unsafe {
            super::delete(&cell_account, db.operator());
        }
        return Ok(());
    }

    let cells = legacy_data.read_cells(&cell_account)?;

    // Make account smaller
    let required_len = StorageCell::required_account_size(cells.len());
    reduce_account_size(&cell_account, required_len, db.operator())?;

    // Fill it with new data
    cell_account.try_borrow_mut_data()?.fill(0);
    super::set_tag(&crate::ID, &cell_account, TAG_STORAGE_CELL)?;

    let mut storage = StorageCell::from_account(&crate::ID, cell_account)?;
    storage.cells_mut().copy_from_slice(&cells);

    Ok(())
}

pub fn update_legacy_accounts(accounts: &AccountsDB) -> Result<()> {
    let keys = KeysCache::new();

    let mut legacy_storage = Vec::with_capacity(accounts.accounts_len());

    for account in accounts {
        if !crate::check_id(account.owner) {
            continue;
        }

        if account.data_is_empty() {
            continue;
        }

        let tag = account.try_borrow_data()?[0];
        match tag {
            LegacyEtherData::TAG => {
                let legacy_data = LegacyEtherData::from_account(&crate::ID, account)?;
                update_ether_account(&legacy_data, accounts, &keys)?;
            }
            LegacyStorageData::TAG => {
                let legacy_data = LegacyStorageData::from_account(&crate::ID, account)?;
                legacy_storage.push(legacy_data);
            }
            _ => {}
        }
    }

    for data in legacy_storage {
        update_storage_account(&data, accounts, &keys)?;
    }

    Ok(())
}
