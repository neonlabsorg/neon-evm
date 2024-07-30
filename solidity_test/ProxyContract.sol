// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Logic contract, which can be upgraded
contract LogicContract {
    // Function to be delegated to
    function doSomething() public pure returns (string memory) {
        return "Doing something!";
    }
}

contract Proxy {
    address public implementation;

    constructor(address _logic) {
        implementation = _logic;
    }

    fallback() external payable {
        // Delegate call to the address stored in the `implementation` variable
        (bool success, bytes memory data) = implementation.delegatecall(msg.data);
        require(success, "Delegatecall failed");
        assembly {
            let size := mload(data)
            returndatacopy(0x0, 0x0, size)
            return(0x0, size)
        }
    }
}
