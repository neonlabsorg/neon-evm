// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./CalledContract2.sol";

contract CallerContract2 {
    // Event declaration
    event CallerEvent(string message, address calledContract);

    // Function to call CalledContract2's emitCalledEvent and emit an event here too
    function callEmitEvent(address _calledContractAddress) public {
        CalledContract2 calledContract = CalledContract2(_calledContractAddress);
        calledContract.emitCalledEvent();
        emit CallerEvent("CallerContract2 function called another contract", _calledContractAddress);
    }
}
