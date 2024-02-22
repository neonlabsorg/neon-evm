// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface ICalledContract {
    function getNumber() external view returns (uint);
    function setNumber(uint _number) external returns (uint);
}

contract CallerContract {
    ICalledContract calledContract;

    constructor(address _calledContractAddress) {
        calledContract = ICalledContract(_calledContractAddress);
    }

    function callGetNumber() public view returns (uint) {
        return calledContract.getNumber();
    }

    // Attempting to call setNumber and return its value.
    // This function will not behave as expected if called as a transaction
    // because return values from state-changing operations are not available to callers.
    function callSetNumber(uint _number) public returns (uint) {
        // Directly returning the value from a state-changing call is not effective in a transactional context.
        return calledContract.setNumber(_number);
    }
}
