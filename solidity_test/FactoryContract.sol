// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import './ChildContract3.sol';

contract FactoryContract {
    event Deployed(address addr, uint256 salt);

    // Function to deploy a ChildContract with CREATE2
    function deployChild(string memory _name, uint256 _salt) public {
        // Bytecode of the ChildContract
        bytes memory bytecode = abi.encodePacked(type(ChildContract3).creationCode, abi.encode(_name));

        address childAddress;

        // Using assembly to call CREATE2
        assembly {
            childAddress := create2(0, add(bytecode, 0x20), mload(bytecode), _salt)
        }

        require(childAddress != address(0), "Failed to deploy the child contract");

        emit Deployed(childAddress, _salt);
    }

    // Function to compute the address of the ChildContract deterministically without deploying
    function computeAddress(string memory _name, uint256 _salt) public view returns (address) {
        bytes memory bytecode = abi.encodePacked(type(ChildContract3).creationCode, abi.encode(_name));
        bytes32 hash = keccak256(
            abi.encodePacked(
                bytes1(0xff),
                address(this),
                _salt,
                keccak256(bytecode)
            )
        );

        // The address can be computed by taking the last 20 bytes of the hash
        return address(uint160(uint256(hash)));
    }
}
