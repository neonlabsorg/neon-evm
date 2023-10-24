#[cfg(any(target_os = "solana", feature = "test-bpf"))]
#[macro_export]
macro_rules! debug_print {
    ($( $args:expr ),*) => {};
}

#[cfg(all(not(target_os = "solana"), feature = "log", not(feature = "test-bpf")))]
#[macro_export]
macro_rules! debug_print {
    ($( $args:expr ),*) => { log::debug!( $( $args ),* ) }
}

#[cfg(all(
    not(target_os = "solana"),
    not(feature = "log"),
    not(feature = "test-bpf")
))]
#[macro_export]
macro_rules! debug_print {
    ($( $args:expr ),*) => {};
}
