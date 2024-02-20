// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ChildContract3 {
    address public creator;
    string public name;

    constructor(string memory _name) {
        creator = msg.sender;
        name = _name;
    }
}
