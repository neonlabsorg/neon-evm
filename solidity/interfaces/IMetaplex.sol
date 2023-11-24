// SPDX-License-Identifier: MIT
pragma solidity 0.8.21;

interface IMetaplex {
    // Set up SPL token metadata for passed SPL token address.
    function createMetadata(bytes32 _mint, string memory _name, string memory _symbol, string memory _uri) external returns(bytes32);
    
    // proof of the Non-Fungibility of the token
    // Note - Decimals of the token has to be 0
    function createMasterEdition(bytes32 mint, uint64 maxSupply) external returns(bytes32);

    // Check if SPL token has been minted
    function isInitialized(bytes32 mint) external view returns(bool);

    // Check if the minted token is an NFT
    function isNFT(bytes32 mint) external view returns(bool);

    // Getter to return SPL token URI
    function uri(bytes32 mint) external view returns(string memory);

    // Getter to return SPL token name
    function name(bytes32 mint) external view returns(string memory);

    // Getter to return SPL token symbol
    function symbol(bytes32 mint) external view returns(string memory);
}