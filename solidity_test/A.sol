// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract A {
    uint256 public number;

    function setNumber(uint256 _number) public returns (uint256) {
        number = _number;
        return number;
    }

    function getNumber() public view returns (uint256) {
        return number;
    }
}
