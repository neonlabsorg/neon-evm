// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import "./openzeppelin-fork/contracts-upgradeable/proxy/utils/Initializable.sol";
import './ERC20ForSPLBackbone.sol';


/// @title ERC20ForSPL
/// @author https://twitter.com/mnedelchev_
/// @notice This contract serves as an interface contract of already deployed SPLToken on Solana. Through this interface Ethereum-like address on Neon EVM chain can apply changes on SPLToken account on Solana.
/// @dev This contract is being used as a BeaconProxy implementation. The Beacon is defined and inherited from ERC20ForSPLBackbone.sol at storage slot 0.
/// @custom:oz-upgrades-unsafe-allow constructor
contract ERC20ForSPL is ERC20ForSPLBackbone, Initializable {
    /// @notice Disabling the initializers to prevent the implementation getting hijacked
    constructor() {
        _disableInitializers();
    }

    /// @notice Method used by the OpenZeppelin's UUPS lib that mimics the functionality of a constructor
    /// @param _tokenMint The Solana-like address of the Token Mint on Solana
    function initialize(bytes32 _tokenMint) public initializer {
        if (!SPL_TOKEN.getMint(_tokenMint).isInitialized) revert InvalidTokenMint();
        if (!METAPLEX.isInitialized(_tokenMint)) revert MissingMetaplex();

        tokenMint = _tokenMint;
    }
}