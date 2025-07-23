use ethnum::U256;
use evm_loader::evm::database::Database;
use evm_loader::executor::{ExecutorStateData, SyncedExecutorState};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

use evm_loader::types::Address;

use crate::commands::get_config::BuildConfigSimulator;
use crate::emulator_platform::EmulatorPlatform;
use crate::NeonResult;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GetStorageAtReturn(pub [u8; 32]);

pub async fn execute(
    rpc: &impl BuildConfigSimulator,
    program_id: &Pubkey,
    address: Address,
    index: U256,
) -> NeonResult<GetStorageAtReturn> {
    let mut platform = EmulatorPlatform::new(rpc, *program_id, &[], &[]).await?;
    let mut executor_data = ExecutorStateData::new();
    let executor = SyncedExecutorState::new(&mut platform, &mut executor_data);

    let value = executor.storage(address, index).await?;
    Ok(GetStorageAtReturn(value))
}
