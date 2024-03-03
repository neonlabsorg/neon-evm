use std::mem::ManuallyDrop;
use std::ptr::{addr_of_mut, copy_nonoverlapping};

use crate::allocator::{acc_allocator, state};

use crate::account_storage::ProgramAccountStorage;
use crate::evm::tracing::NoopEventListener;
use crate::evm::Machine;
use crate::executor::ExecutorState;
use crate::types::{Address, Transaction};
use allocator_api2::boxed::Box;

type EvmBackend<'a, 'r> = ExecutorState<'r, ProgramAccountStorage<'a>>;
type Evm<'a, 'r> = Machine<EvmBackend<'a, 'r>, NoopEventListener>;

#[repr(C)]
pub struct PersistentState<'a, 'r> {
    pub backend: EvmBackend<'a, 'r>,
    pub root_evm: Evm<'a, 'r>,
}

impl<'a, 'r> PersistentState<'a, 'r> {
    pub fn alloc(trx: Transaction, origin: Address, accounts: &'r ProgramAccountStorage<'a>) {
        let boxed_state = {
            Box::new_in(
                // move the ownership of underlying state into the box using into_inner.
                // No destructor shall be invoked as a result.
                ManuallyDrop::into_inner(Self::new(trx, origin, accounts)),
                acc_allocator(),
            )
        };
        unsafe { copy_nonoverlapping(boxed_state.as_ref(), state() as *mut PersistentState, 1) };
        std::mem::forget(boxed_state);
    }

    pub fn new(
        trx: Transaction,
        origin: Address,
        accounts: &'r ProgramAccountStorage<'a>,
    ) -> ManuallyDrop<Self> {
        let mut backend = ExecutorState::new(accounts);
        let root_evm = Machine::new(trx, origin, &mut backend, None::<NoopEventListener>).unwrap();
        ManuallyDrop::new(Self { backend, root_evm })
    }

    pub fn restore(accounts: &'r ProgramAccountStorage<'a>) -> ManuallyDrop<Self> {
        let state_ptr = state();

        // Reinit the reference onto ProgramAccountStorage.
        //
        // N.B. Rust currently does not allow (UB) to implicitly create a ref to
        // an uninitialized or invalid piece of memory.
        // The following seems the least dirty approach to keep the code sound.
        unsafe {
            let backend_ptr = addr_of_mut!((*state_ptr).backend);
            addr_of_mut!((*backend_ptr).backend).write_unaligned(accounts);
        };

        let mut state = unsafe { ManuallyDrop::new(std::ptr::read(state_ptr)) };

        // Reinit buffers.
        let st = &mut *state;
        st.root_evm.reinit(&st.backend);
        state
    }
}
