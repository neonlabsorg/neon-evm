// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

interface IMetaplex {
    /// @notice Pass SPLToken metadata to Metaplex
    function createMetadata(bytes32 _mint, string memory _name, string memory _symbol, string memory _uri) external returns(bytes32);
    
    /// @notice Creates Master Edition account on Solana
    /// @dev  Decimals of the token has to be 0
    function createMasterEdition(bytes32 mint, uint64 maxSupply) external returns(bytes32);

    /// @notice Check if SPLToken has been minted
    function isInitialized(bytes32 mint) external view returns(bool);

    /// @notice Check if the minted token is an NFT
    function isNFT(bytes32 mint) external view returns(bool);

    /// @notice Getter to return SPLToken URI
    function uri(bytes32 mint) external view returns(string memory);

    /// @notice Getter to return SPLToken name
    function name(bytes32 mint) external view returns(string memory);

    /// @notice Getter to return SPLToken symbol
    function symbol(bytes32 mint) external view returns(string memory);
}