// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ArrayAccess {
    uint[] private myArray;
    uint public lastAccessedValue;

    constructor() {
        myArray.push(1); // Add a single element to the array
    }

    // Now `getArrayElement` modifies the state by updating `lastAccessedValue`
    function getArrayElement(uint index) public returns (uint) {
        uint element = myArray[index]; // This line can still cause a panic if `index` is out of bounds
        lastAccessedValue = element; // State modification requires a transaction
        return element;
    }
}
