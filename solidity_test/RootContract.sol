// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IContractA {
    function doSomething() external returns (bool);
}

interface IContractB {
    function doSomething() external returns (bool);
}

contract ContractA is IContractA {
    event ActionA(bool success);

    function doSomething() external override returns (bool) {
        emit ActionA(true);
        return true;
    }
}

contract ContractB is IContractB {
    event ActionB(bool success);

    function doSomething() external override returns (bool) {
        emit ActionB(false);
        revert("ContractB: Call failed");
    }
}

contract RootContract {
    event RootAction(address indexed caller, string action, bool success);

    function callBothContracts(IContractA contractA, IContractB contractB) public {
        bool successA;
        // Attempt to call ContractA
        try contractA.doSomething() {
            emit RootAction(address(contractA), "callContractA", true);
            successA = true;
        } catch {
            emit RootAction(address(contractA), "callContractA", false);
            successA = false;
        }

        // Attempt to call ContractB
        try contractB.doSomething() {
            emit RootAction(address(contractB), "callContractB", true);
        } catch {
            emit RootAction(address(contractB), "callContractB", false);
        }

        // Optional: Logic based on the success of calls
        if (successA) {
            // Logic if ContractA call was successful
        } else {
            // Logic if ContractA call failed
        }
    }
}
