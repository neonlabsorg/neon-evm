// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

interface ISPLToken {
    enum AccountState {
        Uninitialized,
        Initialized,
        Frozen
    }

    struct Account {
        bytes32 mint;
        bytes32 owner;
        uint64 amount;
        bytes32 delegate;
        uint64 delegated_amount;
        bytes32 close_authority;
        AccountState state;
    }

    struct Mint {
        uint64 supply;
        uint8 decimals;
        bool isInitialized;
        bytes32 freezeAuthority;
        bytes32 mintAuthority;
    }

    /// @notice Getter returing the user's ATA in bytes32 format for this SPLToken
    function findAccount(bytes32 salt) external pure returns(bytes32);

    /// @notice Check if account is a system account. System account is not owned by anyone until it's initialized.
    function isSystemAccount(bytes32 account) external view returns(bool);

    /// @notice Return SPLToken account data. This function checks the account is owned by correct SPLToken. Return default not initialized SPLToken account data if corresponded Solana account doesn't exist.
    function getAccount(bytes32 account) external view returns(Account memory);

    /// @notice Return SPLToken mint data. This function checks the mint is owned by correct SPLToken. Returns the default SPLToken mint data which is not initialized if corresponding Solana account doesn't exist.
    function getMint(bytes32 account) external view returns(Mint memory);

    /// @notice Mints new SPL token and returns the Solana token address in bytes32 format.First Solana instruction to call before anything else in the SPL token mint process
    function initializeMint(bytes32 salt, uint8 decimals) external returns(bytes32);

    /// @notice Mints new SPL token and returns the Solana token address in bytes32 format, authority needed. First Solana instruction to call before anything else in the SPL token mint process
    function initializeMint(bytes32 salt, uint8 decimals, bytes32 mint_authority, bytes32 freeze_authority) external returns(bytes32);

    /// @notice Initializing an Associated token account. No signer needed, because parameter bytes32 salt is used as seed. ( createAccountWithSeed instruction )
    function initializeAccount(bytes32 salt, bytes32 mint) external returns(bytes32);

    /// @notice Initializing an Associated token account. Signer needed. ( createAccount instruction )
    function initializeAccount(bytes32 salt, bytes32 mint, bytes32 owner) external returns(bytes32);

    /// @notice  Close a token account
    function closeAccount(bytes32 account) external;

    /// @notice  Mint SPL tokens to an account
    function mintTo(bytes32 mint, bytes32 account, uint64 amount) external;

    /// @notice Burn SPL tokens from an account
    function burn(bytes32 mint, bytes32 account, uint64 amount) external;

    /// @notice Approve a delegate to transfer up to a maximum number of SPL tokens from an account. In Solana you could've a maximum of 1 delegate.
    function approve(bytes32 source, bytes32 target, uint64 amount) external;

    /// @notice Revoke approval for the transfer of SPL tokens from an account
    function revoke(bytes32 source) external;

    /// @notice Transfer SPL tokens from one account to another
    function transfer(bytes32 source, bytes32 target, uint64 amount) external;

    /// @notice Transfer funds from SPLToken accounts owned by Solana user. This method uses PDA[ACCOUNT_SEED_VERSION, b"AUTH", msg.sender, seed] to authorize transfer
    function transferWithSeed(bytes32 seed, bytes32 source, bytes32 target, uint64 amount) external;

    /// @notice Freeze token account
    function freeze(bytes32 mint, bytes32 account) external;

    /// @notice Unfreeze token account
    function thaw(bytes32 mint, bytes32 account) external;
}