use evm_loader::account::Container;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

use crate::emulator_account::SharedAccount;
use crate::NeonResult;
use crate::{rpc::Rpc, NeonError};

use serde_with::{hex::Hex, serde_as, DisplayFromStr};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ContainerStatus {
    Ok,
    Error(String),
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerElement {
    #[serde_as(as = "DisplayFromStr")]
    pub pubkey: Pubkey,
    #[serde_as(as = "Hex")]
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetContainerResponse {
    pub status: ContainerStatus,
    pub accounts: Vec<ContainerElement>,
}

impl GetContainerResponse {
    #[must_use]
    pub fn error(e: impl std::error::Error) -> Self {
        Self {
            status: ContainerStatus::Error(e.to_string()),
            accounts: vec![],
        }
    }

    #[must_use]
    pub const fn new(accounts: Vec<ContainerElement>) -> Self {
        Self {
            status: ContainerStatus::Ok,
            accounts,
        }
    }
}

pub async fn execute(
    rpc: &impl Rpc,
    program_id: Pubkey,
    pubkey: Pubkey,
) -> NeonResult<GetContainerResponse> {
    let Some(account) = rpc.get_account(&pubkey).await? else {
        let error = NeonError::RpcReturnedEmptyAccount(pubkey);
        return Ok(GetContainerResponse::error(error));
    };

    let account = SharedAccount::new(pubkey, &account);
    let container = match Container::from_account(&program_id, account) {
        Ok(container) => container,
        Err(e) => return Ok(GetContainerResponse::error(e)),
    };

    let count = container.count();
    let mut accounts = Vec::with_capacity(count);

    for i in 0..count {
        let pubkey = container.key_at(i).pubkey;
        let data = container.account_data(i).to_vec();

        accounts.push(ContainerElement { pubkey, data });
    }

    Ok(GetContainerResponse::new(accounts))
}
