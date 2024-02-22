// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract CalledContract {
    uint private storedNumber;

    function getNumber() public view returns (uint) {
        return storedNumber;
    }

    // Note: In a blockchain transaction, the return value of a function that changes
    // state is not accessible to the caller. This is designed for illustration.
    function setNumber(uint _number) public returns (uint) {
        storedNumber = _number;
        return storedNumber; // Return the new number
    }
}
