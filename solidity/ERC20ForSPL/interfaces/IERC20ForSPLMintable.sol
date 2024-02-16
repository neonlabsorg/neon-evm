// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

interface IERC20ForSPLMintable {
    function tokenMint() external returns (bytes32);
}