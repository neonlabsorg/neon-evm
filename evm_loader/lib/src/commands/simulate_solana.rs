use crate::{
    rpc::{CachedRpc, Rpc},
    solana_simulator::{SolanaSimulator, SyncState},
    types::SimulateSolanaRequest,
    NeonResult,
};

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use solana_compute_budget::compute_budget::ComputeBudget;
use solana_sdk::{
    account::Account,
    instruction::{Instruction, InstructionError},
    pubkey::Pubkey,
};
use solana_sdk_ids::{bpf_loader, bpf_loader_upgradeable, loader_v4, native_loader};
use std::collections::HashSet;

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct SimulateSolanaResult {
    pub error: Option<InstructionError>,
    pub logs: Vec<String>,
    pub executed_units: u64,
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct SimulateSolanaResponse {
    pub instructions: Vec<SimulateSolanaResult>,
}

fn account_keys(instructions: &[Instruction]) -> HashSet<Pubkey> {
    let mut pubkeys: HashSet<Pubkey> = HashSet::<Pubkey>::new();
    for instruction in instructions {
        pubkeys.insert(instruction.program_id);

        let accounts = instruction.accounts.iter().map(|a| a.pubkey);
        pubkeys.extend(accounts);
    }

    pubkeys
}

fn compute_budget(request: &SimulateSolanaRequest) -> ComputeBudget {
    let compute_unit_limit = request.compute_units.unwrap_or(1_400_000);
    let heap_size = request.heap_size.unwrap_or(256 * 1024);

    ComputeBudget {
        compute_unit_limit,
        heap_size,
        ..Default::default()
    }
}

fn override_accounts(
    simulator: &mut SolanaSimulator,
    solana_keys: &mut HashSet<Pubkey>,
    request: &SimulateSolanaRequest,
) {
    if let Some(program_overrides) = &request.programs_overrides {
        for (pubkey, program) in program_overrides {
            solana_keys.remove(pubkey);
            simulator.add_program(pubkey, &program.elf, &program.loader);
        }
    }

    if let Some(account_overrides) = &request.accounts_overrides {
        for (pubkey, account) in account_overrides {
            if bpf_loader::check_id(&account.owner)
                || bpf_loader_upgradeable::check_id(&account.owner)
                || loader_v4::check_id(&account.owner)
                || native_loader::check_id(&account.owner)
            {
                continue; // Use `program_overrides` for executable accounts
            }

            solana_keys.remove(pubkey);
            simulator.add_account(pubkey, Account::from(account));
        }
    }
}

pub async fn execute(
    rpc: &impl Rpc,
    request: SimulateSolanaRequest,
) -> NeonResult<(SimulateSolanaResponse, SolanaSimulator)> {
    let rpc = CachedRpc::new(rpc);

    let budget = compute_budget(&request);

    let mut simulator = SolanaSimulator::new_with_config(&rpc, budget, SyncState::Yes).await?;

    // Decode instruction
    let mut instructions: Vec<Instruction> = vec![];
    for serialized in request.instructions.iter().cloned() {
        let instruction = serialized.into();
        instructions.push(instruction);
    }

    // Take keys for accounts that should be downloaded from Solana
    let mut solana_keys: HashSet<Pubkey> = account_keys(&instructions);

    // Take override accounts from request, if set
    override_accounts(&mut simulator, &mut solana_keys, &request);

    // Download accounts from Solana
    let solana_keys: Vec<Pubkey> = solana_keys.into_iter().collect();
    simulator.sync_accounts(&rpc, &solana_keys).await?;

    // Process instructions
    let mut results = Vec::new();
    for instruction in instructions {
        let (r, logs) = simulator.process_instruction(&instruction)?;

        results.push(SimulateSolanaResult {
            error: r.raw_result.err(),
            executed_units: r.compute_units_consumed,
            logs,
        });
    }

    Ok((
        SimulateSolanaResponse {
            instructions: results,
        },
        simulator,
    ))
}
