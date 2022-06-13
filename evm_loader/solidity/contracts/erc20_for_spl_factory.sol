// SPDX-License-Identifier: MIT
pragma solidity >=0.5.12;

import './erc20_for_spl.sol';

contract ERC20ForSplFactory {
    address constant precompiled = 0xFf00000000000000000000000000000000000004;

    mapping(bytes32 => address) public getErc20ForSpl;
    address[] public allErc20ForSpl;

    function check_spl_token(bytes32 solana_address) internal view returns (bool) {
        (bool success, bytes memory _dummy) = precompiled.staticcall(abi.encodeWithSignature("check_spl_token(bytes32)", solana_address));
        return (success);
    }

    function allErc20ForSplLength() external view returns (uint) {
        return allErc20ForSpl.length;
    }

    function createErc20ForSpl(string memory name, string memory symbol, bytes32 mint) public returns (address erc20spl) {

        require(getErc20ForSpl[mint] == address(0), 'ERC20 SPL Factory: ERC20_SPL_EXISTS');

        (bool passed) = check_spl_token(mint);
        require(passed, 'ERC20 SPL Factory: SPL TOKEN NOT FOUND');

        bytes memory bytecode = type(ERC20ForSpl).creationCode;
        bytes32 salt = keccak256(abi.encodePacked(mint));
        assembly {
            erc20spl := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }

        ERC20ForSpl(erc20spl).initialize(name, symbol, mint);
        getErc20ForSpl[mint] = erc20spl;
        allErc20ForSpl.push(erc20spl);
        // emit PairCreated(token0, token1, pair, allPairs.length);
    }
}
