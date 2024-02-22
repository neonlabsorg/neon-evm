// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AcceptEtherOnCreation {

    // Event to emit when Ether is received.
    event EtherReceived(address sender, uint amount);

    // The payable constructor allows the contract to accept Ether upon creation.
    // The `payable` keyword is crucial here; it allows the function to receive Ether.
    constructor() payable {
        // Emit an event whenever the contract receives Ether.
        emit EtherReceived(msg.sender, msg.value);
    }

    // Fallback function in case Ether is sent to the contract after it is deployed.
    // This is not necessary for accepting Ether on creation but is a good practice
    // to handle any Ether sent to the contract post-deployment.
    fallback() external payable {
        emit EtherReceived(msg.sender, msg.value);
    }

    // Function to check the contract's balance.
    // This function allows anyone to check the balance of the contract.
    function getBalance() public view returns (uint) {
        return address(this).balance;
    }
}
