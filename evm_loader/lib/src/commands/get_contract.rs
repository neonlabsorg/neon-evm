use evm_loader::{
    account::pda, evm::database::Database, executor::SyncedExecutorState, types::Address,
};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

use crate::{
    commands::get_config::BuildConfigSimulator, emulator_platform::EmulatorPlatform, NeonResult,
};

use serde_with::{hex::Hex, serde_as, DisplayFromStr};

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct GetContractResponse {
    #[serde_as(as = "DisplayFromStr")]
    pub solana_address: Pubkey,
    pub chain_id: Option<u64>,
    #[serde_as(as = "Hex")]
    pub code: Vec<u8>,
}

impl GetContractResponse {
    #[must_use]
    pub const fn new(pubkey: Pubkey, chain_id: Option<u64>, code: Vec<u8>) -> Self {
        Self {
            solana_address: pubkey,
            chain_id,
            code,
        }
    }
}

pub async fn execute(
    rpc: &impl BuildConfigSimulator,
    program_id: &Pubkey,
    addresses: &[Address],
) -> NeonResult<Vec<GetContractResponse>> {
    let mut result = Vec::with_capacity(addresses.len());

    let pubkeys: Vec<_> = addresses
        .iter()
        .map(|a| a.find_solana_address(program_id).0)
        .collect();

    let chains = super::get_config::read_chains(rpc, *program_id).await?;

    let mut platform = EmulatorPlatform::new(rpc, *program_id, &chains, &pubkeys).await?;
    let executor = SyncedExecutorState::new(&mut platform).await;

    for address in addresses.iter().copied() {
        let (pubkey, _) = pda::contract_address(program_id, &address);
        let chain_id = executor.contract_chain_id(address).await.ok();
        let code = executor.use_code(address, <[u8]>::to_vec).await?;

        let response = GetContractResponse::new(pubkey, chain_id, code);
        result.push(response);
    }

    Ok(result)
}
