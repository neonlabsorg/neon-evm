// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract FillArray {
    uint256[5] public numbers;

    constructor() {
        // Initialize the array with 5 values
        numbers[0] = 1;
        numbers[1] = 2;
        numbers[2] = 3;
        numbers[3] = 4;
        numbers[4] = 5;
    }

    // Function to fill the array with specific values
    function fillArray(uint256[5] memory newValues) public {
        numbers = newValues;
    }

    // Function to get the array values
    function getArray() public view returns (uint256[5] memory) {
        return numbers;
    }
}
