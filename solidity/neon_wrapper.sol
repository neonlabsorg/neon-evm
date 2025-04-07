// SPDX-License-Identifier: MIT
pragma solidity >=0.7.6;

interface INeonWithdraw {
    function withdraw(bytes32) external payable returns (bool);

    function withdraw_on_chain(uint64 chain_id, bytes32 to, uint256 amount) external returns (bool);
}

contract NeonToken {
    INeonWithdraw constant NeonPrecompiled =
        INeonWithdraw(0xFF00000000000000000000000000000000000003);

    uint64 chain_id;

    constructor() {
        uint bigChainId;
        assembly {
            bigChainId := chainid()
        }
        chain_id = uint64(bigChainId);
    }

    function withdraw(bytes32 solana_address) external payable {
        NeonPrecompiled.withdraw_on_chain(chain_id, solana_address, msg.value);
    }
}
