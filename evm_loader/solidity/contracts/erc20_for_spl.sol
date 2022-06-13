// SPDX-License-Identifier: MIT

pragma solidity >=0.5.16;


interface IERC20 {
    function decimals() external view returns (uint8);
    function totalSupply() external view returns (uint256);
    function balanceOf(address who) external view returns (uint256);
    function allowance(address owner, address spender) external view returns (uint256);
    function transfer(address to, uint256 value) external returns (bool);
    function approve(address spender, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    
    
    function approveSolana(bytes32 spender, uint64 value) external returns (bool);
    event ApprovalSolana(address indexed owner, bytes32 indexed spender, uint64 value);
}



contract ERC20ForSpl {
    address constant precompiled = 0xff00000000000000000000000000000000000001;

    address public factory;
    string  public name;
    string  public symbol;
    bytes32 public tokenMint;

    constructor() {
        factory = msg.sender;
    }

    // called once by the factory at time of deployment
    function initialize(string memory _name, string memory _symbol, bytes32 _tokenMint) external {
        require(msg.sender == factory, 'Neon ERC20 Factory: FORBIDDEN'); // sufficient check
        name = _name;
        symbol = _symbol;
        tokenMint = _tokenMint;
    }

    fallback() external {
        bytes memory call_data = abi.encodePacked(tokenMint, msg.data);
        (bool success, bytes memory result) = precompiled.delegatecall(call_data);

        require(success, string(result));

        assembly {
            return(add(result, 0x20), mload(result))
        }
    }
}
