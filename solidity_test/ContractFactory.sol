// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ChildContract {
    uint public storedValue;

    constructor(uint _value) {
        storedValue = _value;
    }
}

contract ContractFactory {
    ChildContract[] public childContracts;

    function createChildContract(uint _value) public {
        ChildContract child = new ChildContract(_value);
        childContracts.push(child);
    }

    function getDeployedChildContracts() public view returns (ChildContract[] memory) {
        return childContracts;
    }
}
