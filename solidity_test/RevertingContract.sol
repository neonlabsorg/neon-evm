// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RevertingContract {
    uint256 public storedNumber;

    // Function to set a number, then revert if the input is greater than 10
    function setNumber(uint256 _number) public {
        // This line sets the storage value
        storedNumber = _number;

        // Revert if _number is greater than 10
        // The transaction will be reverted, and the change to storedNumber won't be saved
        require(_number <= 10, "Number is greater than 10, transaction reverted.");

        // If the require condition is not met (i.e., _number is less than or equal to 10),
        // execution continues past this point without reverting
    }
}
