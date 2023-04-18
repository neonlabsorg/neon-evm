use std::convert::TryInto;

use ethnum::U256;
use solana_sdk::pubkey::Pubkey;

use evm_loader::account::EthereumAccount;
use evm_loader::{
    account::{ether_storage::EthereumStorageAddress, EthereumStorage},
    config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT,
    types::Address,
};

use crate::{
    account_storage::{account_info, EmulatorAccountStorage},
    errors::NeonCliError,
    rpc::Rpc,
};

pub fn execute(
    rpc_client: &dyn Rpc,
    evm_loader: &Pubkey,
    ether_address: Address,
    index: &U256,
) -> Result<[u8; 32], NeonCliError> {
    let value = if let (solana_address, Some(mut account)) =
        EmulatorAccountStorage::get_account_from_solana(rpc_client, evm_loader, &ether_address)
    {
        let info = account_info(&solana_address, &mut account);

        let account_data = EthereumAccount::from_account(evm_loader, &info)?;
        if let Some(contract) = account_data.contract_data() {
            if *index < U256::from(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT) {
                let index: usize = index.as_usize() * 32;
                contract.storage()[index..index + 32].try_into().unwrap()
            } else {
                let subindex = (*index & 0xFF).as_u8();
                let index = *index & !U256::new(0xFF);

                let address =
                    EthereumStorageAddress::new(evm_loader, account_data.info.key, &index);

                if let Ok(mut account) = rpc_client.get_account(address.pubkey()) {
                    if solana_sdk::system_program::check_id(&account.owner) {
                        <[u8; 32]>::default()
                    } else {
                        let account_info = account_info(address.pubkey(), &mut account);
                        let storage =
                            EthereumStorage::from_account(evm_loader, &account_info)?;
                        if (storage.address != ether_address)
                            || (storage.index != index)
                            || (storage.generation != account_data.generation)
                        {
                            <[u8; 32]>::default()
                        } else {
                            storage.get(subindex)
                        }
                    }
                } else {
                    <[u8; 32]>::default()
                }
            }
        } else {
            <[u8; 32]>::default()
        }
    } else {
        <[u8; 32]>::default()
    };

    Ok(value)
}
