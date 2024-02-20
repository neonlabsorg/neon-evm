// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./B.sol";

contract C {
    B public b;
    uint256 public result;

    constructor(address _b) {
        b = B(_b);
    }

    function doubleNumberFromB(uint256 _number) public returns (uint256) {
        uint256 numberFromB = b.addFiveToNumberFromA(_number); // Assuming result is accessible
        result = numberFromB * 2;
        b.addFiveToNumberFromA(_number); // Optionally trigger B to update its result based on A
        return result;
    }
}
