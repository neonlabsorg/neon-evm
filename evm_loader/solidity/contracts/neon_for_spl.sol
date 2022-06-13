// SPDX-License-Identifier: MIT
pragma solidity >=0.5.12;

contract NeonForSpl {
    address constant precompiled = 0xFF00000000000000000000000000000000000003;

    function withdraw(bytes32 spender) public payable {
        (bool success, bytes memory returnData) = precompiled.delegatecall(abi.encodeWithSignature("withdraw(bytes32)", spender));
        require(success, string(returnData));
    }
}