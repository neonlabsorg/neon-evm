use std::cell::{Ref, RefMut};
use std::mem::size_of;

use super::treasury::Treasury;
use super::{
    pda, program, AccountHeader, AccountRead, AccountWrite, Balance, Operator, ACCOUNT_PREFIX_LEN,
    TAG_TRANSACTION_TREE,
};
use crate::account::Account;
use crate::config::{
    BASE_ITERATIVE_TRANSACTION_COST, TREE_ACCOUNT_DESTROY_FEE, TREE_ACCOUNT_FINISH_TRANSACTION_FEE,
    TREE_ACCOUNT_TIMEOUT,
};
use crate::error::{Error, Result};
use crate::evm::ExitStatus;
use crate::types::{Address, ScheduledTransaction};
use ethnum::U256;
use solana_program::{
    account_info::AccountInfo, clock::Clock, pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Status {
    Failed = 0x00,
    Success = 0x01,
    Skipped = 0x02,
    InProgress = 0x03,
    #[default]
    NotStarted = 0xFF,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct Node {
    pub status: Status,

    pub result_hash: [u8; 32],
    pub transaction_hash: [u8; 32],

    pub sender: Address,
    pub gas_limit: U256,
    pub value: U256,

    pub child_transaction: u16,
    pub success_execute_limit: u16,
    pub parent_count: u16,
}
const _: () = assert!(std::mem::size_of::<Node>() == 155);

pub const NO_CHILD_TRANSACTION: u16 = u16::MAX;

#[repr(C, packed)]
pub struct HeaderV0 {
    payer: Address,
    last_slot: u64,
    chain_id: u64,
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
    balance: U256,
    last_index: u16,
}
const _: () = assert!(std::mem::size_of::<HeaderV0>() == 134);

impl AccountHeader for HeaderV0 {
    const VERSION: u8 = 0;
}

// Set the last version of the Header struct here
// and change the `header_size` and `header_upgrade` functions
pub type Header = HeaderV0;

pub struct NodeInitializer {
    pub transaction_hash: [u8; 32],
    pub sender: Address,
    pub child: u16,
    pub success_execute_limit: u16,
    pub gas_limit: U256,
    pub value: U256,
}

pub struct TreeInitializer {
    pub payer: Address,
    pub nonce: u64,
    pub chain_id: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub nodes: Vec<NodeInitializer>,
}

pub struct TransactionTree<T> {
    account: T,
}

impl TransactionTree<()> {
    #[must_use]
    pub const fn required_account_size(transactions: usize) -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>() + transactions * size_of::<Node>()
    }

    #[must_use]
    pub fn prepare_exit_status(result: &ExitStatus) -> (Status, solana_program::keccak::Hash) {
        use solana_program::keccak::hash as keccak256;

        let (status, result_hash) = match result {
            ExitStatus::Stop | ExitStatus::Suicide => (Status::Success, keccak256(&[])),
            ExitStatus::Return(result) => (Status::Success, keccak256(result)),
            ExitStatus::Revert(result) => (Status::Failed, keccak256(result)),
            ExitStatus::Cancel => (Status::Failed, keccak256(&[])),
            ExitStatus::Interrupted(_) | ExitStatus::StepLimit => unreachable!(),
        };

        (status, result_hash)
    }
}

impl<'a> TransactionTree<AccountInfo<'a>> {
    pub fn from_account_info(program_id: &Pubkey, account: &AccountInfo<'a>) -> Result<Self> {
        let account = account.clone();
        Self::from_account(program_id, account)
    }

    pub fn create(
        init: TreeInitializer,
        mut account: AccountInfo<'a>,
        system: &program::System<'a>,
        treasury: &Treasury<'a>,
        destroy_fee_payer: &Operator<'a>,
        rent: &Rent,
        clock: &Clock,
    ) -> Result<Self> {
        const MIN_FEE_PER_GAS: U256 = U256::new(1_100_000_000);
        const MIN_GAS_LIMIT: U256 = U256::new(BASE_ITERATIVE_TRANSACTION_COST as u128);
        const TREE_ACCOUNT_MAX_NODES: usize = 16;

        // Validate account
        let (pubkey, bump) = pda::tree_account(&crate::ID, &init.payer, init.chain_id, init.nonce);
        if account.key != &pubkey {
            return Err(Error::AccountInvalidKey(*account.key, pubkey));
        }

        if !account.is_system_owned() {
            return Err(Error::TreeAccountAlreadyExists);
        }

        if init.max_fee_per_gas < MIN_FEE_PER_GAS {
            // Require at least 1.1 to 1.1 ratio to operator spending
            // 1.1 GAlan in gas equals to 1.1 lamport
            return Err(Error::TreeAccountInvalidFeePerGas);
        }

        let nodes = init.nodes;
        if nodes.len() > TREE_ACCOUNT_MAX_NODES {
            return Err(Error::TreeAccountTxInvalidTooMuchNodes);
        }

        let mut parent_counts = vec![0_u16; nodes.len()];

        for (i, node) in nodes.iter().enumerate() {
            if node.gas_limit < MIN_GAS_LIMIT {
                // Require at least 35_000 gas limit to cover operator spending
                return Err(Error::TreeAccountInvalidGasLimit);
            }

            if node.child == NO_CHILD_TRANSACTION {
                continue;
            }

            if node.child as usize >= nodes.len() {
                return Err(Error::TreeAccountTxInvalidChildIndex);
            }
            if node.child as usize <= i {
                // Child transaction should be after parent transaction
                return Err(Error::TreeAccountTxInvalidChildIndex);
            }

            let parent_count = &mut parent_counts[node.child as usize];
            *parent_count = parent_count
                .checked_add(1)
                .ok_or(Error::TreeAccountTxInvalidParentCount)?;
        }

        for (node, parent_count) in nodes.iter().zip(&parent_counts) {
            if node.success_execute_limit > *parent_count {
                return Err(Error::TreeAccountTxInvalidSuccessLimit);
            }
        }

        // Create account
        let seeds: &[&[u8]] = pda::tree_account_seeds!(init, bump);
        let space = TransactionTree::required_account_size(nodes.len());

        system.create_pda_account_with_treasury_payer(
            &crate::ID,
            treasury,
            &account,
            seeds,
            space,
            rent,
        )?;

        let nodes_len = nodes.len() as u64;
        let fee = TREE_ACCOUNT_DESTROY_FEE + (nodes_len * TREE_ACCOUNT_FINISH_TRANSACTION_FEE);
        system.transfer(destroy_fee_payer, &account, fee)?;

        // Init data
        account.write_tag(TAG_TRANSACTION_TREE, Header::VERSION)?;
        account.write_header(HeaderV0 {
            payer: init.payer,
            last_slot: clock.slot,
            chain_id: init.chain_id,
            max_fee_per_gas: init.max_fee_per_gas,
            max_priority_fee_per_gas: init.max_priority_fee_per_gas,
            balance: U256::ZERO,
            last_index: nodes.len().try_into()?,
        });

        let mut tree = Self { account };

        let init_nodes = nodes.into_iter().zip(parent_counts);
        for (node, (init, parent_count)) in tree.nodes_mut().iter_mut().zip(init_nodes) {
            node.status = Status::NotStarted;
            node.result_hash = [0; 32];
            node.transaction_hash = init.transaction_hash;
            node.sender = init.sender;
            node.gas_limit = init.gas_limit;
            node.value = init.value;
            node.child_transaction = init.child;
            node.success_execute_limit = init.success_execute_limit;
            node.parent_count = parent_count;
        }

        Ok(tree)
    }

    pub fn destroy(self, operator: &Operator, treasury: &Treasury<'a>) -> Result<()> {
        let clock = Clock::get()?;

        if !self.can_be_destroyed(&clock) {
            return Err(Error::TreeAccountNotReadyForDestruction);
        }

        let account_info = self.account;

        **operator.lamports.borrow_mut() += TREE_ACCOUNT_DESTROY_FEE;
        **account_info.lamports.borrow_mut() -= TREE_ACCOUNT_DESTROY_FEE;

        unsafe { super::delete_with_treasury(&account_info, treasury) }
    }

    pub fn start_transaction(&mut self, tx: &dyn ScheduledTransaction) -> Result<()> {
        self.validate_transaction(tx)?;
        let mut node = self.node_mut(tx.index());

        if node.status != Status::NotStarted {
            return Err(Error::TreeAccountTxInvalidStatus);
        }
        if node.parent_count != 0 {
            return Err(Error::TreeAccountTxInvalidParentCount);
        }
        if node.success_execute_limit != 0 {
            return Err(Error::TreeAccountTxInvalidSuccessLimit);
        }

        node.status = Status::InProgress;
        std::mem::drop(node);

        let clock = Clock::get()?;
        self.update_last_slot(&clock);

        Ok(())
    }

    pub fn skip_transaction(&mut self, tx: &dyn ScheduledTransaction) -> Result<()> {
        self.validate_transaction(tx)?;
        let mut node = self.node_mut(tx.index());

        if node.status != Status::NotStarted {
            return Err(Error::TreeAccountTxInvalidStatus);
        }
        if node.parent_count != 0 {
            return Err(Error::TreeAccountTxInvalidParentCount);
        }
        if node.success_execute_limit == 0 {
            // Transaction need to be started
            return Err(Error::TreeAccountTxInvalidSuccessLimit);
        }

        node.status = Status::Skipped;

        let child_index = node.child_transaction;
        std::mem::drop(node);

        let clock = Clock::get()?;
        self.update_last_slot(&clock);

        self.decrease_parent_count(child_index, Status::Skipped);

        Ok(())
    }

    pub fn end_transaction(
        &mut self,
        hash: &[u8; 32],
        result: (Status, solana_program::keccak::Hash),
        operator: &Operator<'a>,
    ) -> Result<()> {
        use solana_program::keccak::Hash;

        let index = self.find_node(hash)?;
        let mut node = self.node_mut(index);

        if node.status != Status::InProgress {
            return Err(Error::TreeAccountTxInvalidStatus);
        }

        let (status, Hash(result_hash)) = result;

        node.status = status;
        node.result_hash = result_hash;

        let child_index = node.child_transaction;
        std::mem::drop(node);

        let clock = Clock::get()?;
        self.update_last_slot(&clock);

        self.decrease_parent_count(child_index, status);
        self.pay_for_end_transaction(operator)
    }

    fn pay_for_end_transaction(&self, operator: &Operator<'a>) -> Result<()> {
        let rent = Rent::get()?;
        let minimum_balance = rent.minimum_balance(self.account.data_len());

        let available_lamports = self
            .account
            .lamports()
            .saturating_sub(minimum_balance)
            .saturating_sub(TREE_ACCOUNT_DESTROY_FEE);

        if available_lamports < TREE_ACCOUNT_FINISH_TRANSACTION_FEE {
            return Ok(()); // Not enough funds. This could happen if working with old tree account.
        }

        let account_info = &self.account;

        **account_info.lamports.borrow_mut() -= TREE_ACCOUNT_FINISH_TRANSACTION_FEE;
        **operator.lamports.borrow_mut() += TREE_ACCOUNT_FINISH_TRANSACTION_FEE;

        Ok(())
    }
}

impl<T: AccountRead> TransactionTree<T> {
    pub fn from_account(program_id: &Pubkey, account: T) -> Result<Self> {
        account.validate_tag(program_id, TAG_TRANSACTION_TREE)?;

        Ok(Self { account })
    }

    #[must_use]
    pub fn is_in_progress(&self) -> bool {
        self.nodes()
            .iter()
            .any(|n| matches!(n.status, Status::InProgress))
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.nodes()
            .iter()
            .all(|n| !matches!(n.status, Status::InProgress | Status::NotStarted))
    }

    #[must_use]
    pub fn is_not_started(&self) -> bool {
        self.nodes()
            .iter()
            .all(|n| matches!(n.status, Status::NotStarted))
    }

    #[must_use]
    pub fn can_be_destroyed(&self, clock: &Clock) -> bool {
        if self.balance() != U256::ZERO {
            return false;
        }

        if self.is_in_progress() {
            return false;
        }

        if self.last_slot() < clock.slot.saturating_sub(TREE_ACCOUNT_TIMEOUT) {
            return true;
        }

        self.is_complete()
    }

    fn validate_transaction(&self, tx: &dyn ScheduledTransaction) -> Result<()> {
        let tx_chain_id = tx
            .chain_id()
            .expect("Scheduled transaction must have chain_id");

        let (pubkey, _) = pda::tree_account(&crate::ID, tx.payer(), tx_chain_id, tx.nonce());
        if pubkey != self.account.pubkey() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        if tx_chain_id != self.chain_id() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        if tx.index() as usize >= self.nodes().len() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        let node = self.node(tx.index());
        if &node.transaction_hash != tx.hash() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        if &node.sender != tx.sender().unwrap_or_else(|| tx.payer()) {
            return Err(Error::TreeAccountTxInvalidData);
        }

        let gas_limit = node.gas_limit; // Copy from unaligned
        if gas_limit != tx.gas_limit() {
            return Err(Error::TreeAccountTxInvalidData);
        }
        let value = node.value;
        if value != tx.value() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        if tx.payer() != &self.payer() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        if tx.max_fee_per_gas() != self.max_fee_per_gas() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        if tx.max_priority_fee_per_gas() != self.max_priority_fee_per_gas() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        // We don't support intents at the moment
        if tx.intent().is_some() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        if !tx.intent_call_data().is_empty() {
            return Err(Error::TreeAccountTxInvalidData);
        }

        Ok(())
    }

    #[must_use]
    pub fn payer(&self) -> Address {
        let header: Ref<HeaderV0> = self.account.header();
        header.payer
    }

    #[must_use]
    pub fn last_slot(&self) -> u64 {
        let header: Ref<HeaderV0> = self.account.header();
        header.last_slot
    }

    #[must_use]
    pub fn chain_id(&self) -> u64 {
        let header: Ref<HeaderV0> = self.account.header();
        header.chain_id
    }

    #[must_use]
    pub fn max_fee_per_gas(&self) -> U256 {
        let header: Ref<HeaderV0> = self.account.header();
        header.max_fee_per_gas
    }

    #[must_use]
    pub fn max_priority_fee_per_gas(&self) -> U256 {
        let header: Ref<HeaderV0> = self.account.header();
        header.max_priority_fee_per_gas
    }

    pub fn gas_limit(&self, transaction_hash: &[u8; 32]) -> Result<U256> {
        let index = self.find_node(transaction_hash)?;
        let node = self.node(index);
        Ok(node.gas_limit)
    }

    #[must_use]
    pub fn total_gas_limit(&self) -> U256 {
        self.nodes()
            .iter()
            .fold(U256::ZERO, |v, node| v.saturating_add(node.gas_limit))
    }

    #[must_use]
    pub fn total_value(&self) -> U256 {
        self.nodes()
            .iter()
            .fold(U256::ZERO, |v, node| v.saturating_add(node.value))
    }

    #[must_use]
    pub fn balance(&self) -> U256 {
        let header: Ref<HeaderV0> = self.account.header();
        header.balance
    }

    #[must_use]
    pub fn last_index(&self) -> u16 {
        let header: Ref<HeaderV0> = self.account.header();
        header.last_index
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    fn header_size(&self) -> usize {
        match self.account.header_version() {
            0 | 1 => size_of::<HeaderV0>(),
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }
    }

    fn nodes_offset(&self) -> usize {
        ACCOUNT_PREFIX_LEN + self.header_size()
    }

    #[must_use]
    pub fn nodes(&self) -> Ref<[Node]> {
        let nodes_offset = self.nodes_offset();

        let data = self.account.data();
        let data = Ref::map(data, |d| &d[nodes_offset..]);

        Ref::map(data, |bytes| {
            const { assert!(std::mem::align_of::<Node>() == 1) };
            assert_eq!(bytes.len() % size_of::<Node>(), 0);

            // SAFETY: Node has the same alignment as bytes
            unsafe {
                let ptr = bytes.as_ptr().cast::<Node>();
                let len = bytes.len() / size_of::<Node>();
                std::slice::from_raw_parts(ptr, len)
            }
        })
    }

    #[must_use]
    pub fn node(&self, index: u16) -> Ref<Node> {
        let nodes = self.nodes();
        Ref::map(nodes, |nodes| &nodes[index as usize])
    }

    pub fn find_node(&self, hash: &[u8; 32]) -> Result<u16> {
        let nodes = self.nodes();
        let index = nodes
            .iter()
            .position(|node| &node.transaction_hash == hash)
            .ok_or(Error::TreeAccountTxNotFound)?;

        let index: u16 = index.try_into()?;
        Ok(index)
    }
}

impl<T: Account> TransactionTree<T> {
    pub fn update_last_slot(&mut self, clock: &Clock) {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();
        header.last_slot = clock.slot;
    }

    pub fn withdraw(&mut self, target: &mut Balance<impl Account>) -> Result<()> {
        assert_eq!(self.chain_id(), target.chain_id());
        assert_eq!(self.payer(), target.address());

        let value = self.balance();

        self.burn(value)?;
        target.mint(value)
    }

    pub fn burn(&mut self, value: U256) -> Result<()> {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();

        header.balance = header
            .balance
            .checked_sub(value)
            .ok_or(Error::InsufficientBalance(
                header.payer,
                header.chain_id,
                value,
            ))?;

        Ok(())
    }

    pub fn mint(&mut self, value: U256) -> Result<()> {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();

        header.balance = header
            .balance
            .checked_add(value)
            .ok_or(Error::IntegerOverflow)?;

        Ok(())
    }

    pub fn burn_gas(&mut self, gas: U256, gas_price: U256) -> Result<()> {
        assert_eq!(self.max_fee_per_gas(), gas_price);

        let Some(tokens) = gas.checked_mul(gas_price) else {
            return Err(Error::IntegerOverflow);
        };
        self.burn(tokens)
    }

    pub fn refund_gas(&mut self, gas: U256, gas_price: U256) -> Result<()> {
        assert_eq!(self.max_fee_per_gas(), gas_price);

        let Some(tokens) = gas.checked_mul(gas_price) else {
            return Err(Error::IntegerOverflow);
        };
        self.mint(tokens)
    }

    pub fn increment_last_index(&mut self) -> Result<()> {
        let mut header: RefMut<HeaderV0> = self.account.header_mut();
        header.last_index = header
            .last_index
            .checked_add(1)
            .ok_or(Error::TreeAccountLastIndexOverflow)?;

        Ok(())
    }

    #[must_use]
    pub fn nodes_mut(&mut self) -> RefMut<[Node]> {
        let nodes_offset = self.nodes_offset();

        let data = self.account.data_mut();
        let data = RefMut::map(data, |d| &mut d[nodes_offset..]);

        RefMut::map(data, |bytes| {
            const { assert!(std::mem::align_of::<Node>() == 1) };
            assert_eq!(bytes.len() % size_of::<Node>(), 0);

            // SAFETY: Node has the same alignment as bytes
            unsafe {
                let ptr = bytes.as_mut_ptr().cast::<Node>();
                let len = bytes.len() / size_of::<Node>();
                std::slice::from_raw_parts_mut(ptr, len)
            }
        })
    }

    #[must_use]
    pub fn root_trx_hash(&self) -> [u8; 32] {
        self.node(0).transaction_hash
    }

    #[must_use]
    pub fn node_mut(&mut self, index: u16) -> RefMut<Node> {
        let nodes = self.nodes_mut();
        RefMut::map(nodes, |nodes| &mut nodes[index as usize])
    }

    fn decrease_parent_count(&mut self, index: u16, parent_status: Status) {
        if index == NO_CHILD_TRANSACTION {
            return;
        }

        let mut child = self.node_mut(index);
        let new_parent_count = child.parent_count.checked_sub(1);
        child.parent_count = new_parent_count.unwrap(); // Parent count is calculated by us when tree is created. If code is correct, this should never panic

        if parent_status == Status::Success {
            child.success_execute_limit = child.success_execute_limit.saturating_sub(1);
        }
    }
}
