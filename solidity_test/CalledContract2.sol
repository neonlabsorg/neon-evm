// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract CalledContract2 {
    // Event declaration
    event CalledEvent(string message);

    // Function that emits an event
    function emitCalledEvent() public {
        emit CalledEvent("CalledContract2 function was called");
    }
}
