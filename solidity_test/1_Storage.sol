// SPDX-License-Identifier: GPL-3.0

pragma solidity >=0.8.2 <0.9.0;

/**
 * @title Storage
 * @dev Store & retrieve value in a variable
 * @custom:dev-run-script ./scripts/deploy_with_ethers.ts
 */
contract Storage {

    uint256 number;
    uint256 balance;

    /**
     * @dev Store value in variable
     * @param num value to store
     */
    function store(uint256 num) public {
        number = num;
    }

    /**
     * @dev Return value
     * @return value of 'number'
     */
    function retrieve() public view returns (uint256){
        return number;
    }

    function increment() public {
        number += 1;
    }

    function incrementWithReturn() public returns (uint256) {
        number += 1;
        return number;
    }

    function incrementAndReturn(uint256 input) pure public returns (uint256) {
        input += 1;
        return input;
    }

    function incrementWithInput(uint256 input) public returns (uint256) {
        number += input;
        return number;
    }

    function finalize() public {
        address payable addr = payable(address(bytes20(bytes("0x82211934C340b29561381392348D48413E15ADC8"))));
        selfdestruct(addr);
    }

    function storeBalanceIncremented(address addr) public {
        balance = addr.balance + 1;
    }

    function getBalance() view public returns (uint256) {
        return balance;
    }
}
