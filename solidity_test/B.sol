// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./A.sol";

contract B {
    A public a;
    uint256 public result;

    constructor(address _a) {
        a = A(_a);
    }

    function addFiveToNumberFromA(uint256 _number) public returns (uint256) {
        uint256 numberFromA = a.setNumber(_number);
        result = numberFromA + 5;
        a.setNumber(result); // Optionally update A's number as well
        return result;
    }
}
