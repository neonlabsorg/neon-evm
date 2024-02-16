// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import './interfaces/ISPLToken.sol';
import './interfaces/IMetaplex.sol';


/// @title ERC20ForSPLBackbone
/// @author https://twitter.com/mnedelchev_
/// @notice This contract serves as a backbone contract for both ERC20ForSPL and ERC20ForSPLMintable smart contracts. ERC20ForSPLBackbone contains the core logic of the ERC20ForSPL meanwhile in ERC20ForSPL and ERC20ForSPLMintable smart contracts 
/// @dev This contract is part of a BeaconProxy contract. The storage is defined in the following way:
/// @dev Storage slot 0 - taken by the Beacon address.
/// @dev Every next slot is defined by the needs of the ERC20ForSPL standard.
contract ERC20ForSPLBackbone {
    ISPLToken public constant SPL_TOKEN = ISPLToken(0xFf00000000000000000000000000000000000004);
    IMetaplex public constant METAPLEX = IMetaplex(0xff00000000000000000000000000000000000005);
    
    address public beacon;
    bytes32 public tokenMint;
    mapping(address => mapping(address => uint256)) private _allowances;

    event Transfer(address indexed from, address indexed to, uint256 amount);
    event Approval(address indexed owner, address indexed spender, uint256 amount);
    event ApprovalSolana(address indexed owner, bytes32 indexed spender, uint64 amount);
    event TransferSolana(address indexed from, bytes32 indexed to, uint64 amount);

    error EmptyAddress();
    error InvalidSystemAccount();
    error InvalidAllowance();
    error AmountExceedsBalance();
    error MissingMetaplex();
    error InvalidTokenMint();
    error AmountExceedsUint64();

    /// @notice Returns the name of the SPLToken, the name is stored in Metaplex protocol
    function name() public view returns (string memory) {
        return METAPLEX.name(tokenMint);
    }

    /// @notice Returns the symbol of the SPLToken, the symbol is stored in Metaplex protocol
    function symbol() public view returns (string memory) {
        return METAPLEX.symbol(tokenMint);
    }

    /// @notice Returns the decimals of the SPLToken
    function decimals() public view returns (uint8) {
        return SPL_TOKEN.getMint(tokenMint).decimals;
    }

    /// @notice Returns the totalSupply of the SPLToken
    function totalSupply() public view returns (uint256) {
        return SPL_TOKEN.getMint(tokenMint).supply;
    }

    /// @notice Returns the SPLToken balance of an address
    /// @dev Unlike typical ERC20 the balances of ERC20ForSPL are actually stored on Solana, this standard doesn't have balances mapping
    function balanceOf(address who) public view returns (uint256) {
        return SPL_TOKEN.getAccount(solanaAccount(who)).amount;
    }

    /// @notice Returns the allowances made to Ethereum-like addresses
    function allowance(address owner, address spender) public view returns (uint256) {
        return _allowances[owner][spender];
    }

    /// @notice Converts an address to uint and then converts uint to bytes32
    function _salt(address account) internal pure returns (bytes32) {
        return bytes32(uint256(uint160(account)));
    }

    /// @notice Returns the Solana-like address which is binded to the Ethereum-like address
    /// @dev When an address interacts for the first time with ERC20ForSPL behind the scenes there is Solana account creation which is binded to the Ethereum-like address used on Neon chain
    function solanaAccount(address account) public pure returns (bytes32) {
        return SPL_TOKEN.findAccount(_salt(account));
    }

    /// @notice Returns the allowances made to Solana-like addresses
    /// @dev Solana architecture is a bit different compared to Ethereum and we can actually have only 1 allowed address at a time. Every new approval overwrites the previous one
    function getAccountDelegateData(address who) public view returns(bytes32, uint64) {
        ISPLToken.Account memory account = SPL_TOKEN.getAccount(solanaAccount(who));
        return (account.delegate, account.delegated_amount);
    }

    /// @notice ERC20's approve method
    /// @custom:getter allowance
    function approve(address spender, uint256 amount) public returns (bool) {
        _approve(msg.sender, spender, amount);
        return true;
    }

    /// @notice ERC20's transfer method
    /// @custom:getter balanceOf
    function transfer(address to, uint256 amount) public returns (bool) {
        _transfer(msg.sender, to, amount);
        return true;
    }

    /// @notice ERC20's transferFrom method. Before calling this method the from address has to approve the msg.sender to manage his tokens
    /// @custom:getter balanceOf
    function transferFrom(address from, address to, uint256 amount) public returns (bool) {
        _spendAllowance(from, msg.sender, amount);
        _transfer(from, to, amount);
        return true;
    }

    /// @notice ERC20's burn method
    /// @custom:getter balanceOf
    function burn(uint256 amount) public returns (bool) {
        _burn(msg.sender, amount);
        return true;
    }

    /// @notice ERC20's burnFrom method. Similar to transferFrom function, this method requires the from address to approve the msg.sender first, before calling burnFrom function
    /// @custom:getter balanceOf
    function burnFrom(address from, uint256 amount) public returns (bool) {
        _spendAllowance(from, msg.sender, amount);
        _burn(from, amount);
        return true;
    }

    /// @notice ERC20ForSPL's approve method
    /// @dev With ERC20ForSPL standard we can also make approvals on Solana-like addresses. These type of records are being stored directly on Solana and they're not being recorded inside the _allowances mapping
    /// @param spender The Solana-like address in bytes32 of the spender
    /// @param amount The amount to be managed by the spender
    /// @custom:getter getAccountDelegateData
    function approveSolana(bytes32 spender, uint64 amount) public returns (bool) {
        address from = msg.sender;
        bytes32 fromSolana = solanaAccount(from);

        if (amount > 0) {
            SPL_TOKEN.approve(fromSolana, spender, amount);
        } else {
            SPL_TOKEN.revoke(fromSolana);
        }

        emit Approval(from, address(0), amount);
        emit ApprovalSolana(from, spender, amount);
        return true;
    }

    /// @notice ERC20ForSPL's transfer method
    /// @dev With ERC20ForSPL standard we can also make transfers directly to Solana-like addresses. Balances data is being stored directly on Solana
    /// @param to The Solana-like address in bytes32 of the receiver
    /// @param amount The amount to be transfered to the receiver
    /// @custom:getter balanceOf
    function transferSolana(bytes32 to, uint64 amount) public returns (bool) {
        address from = msg.sender;
        bytes32 fromSolana = solanaAccount(from);

        SPL_TOKEN.transfer(fromSolana, to, uint64(amount));

        emit Transfer(from, address(0), amount);
        emit TransferSolana(from, to, amount);
        return true;
    }

    /// @notice Calling method claimTo with msg.sender to parameter
    /// @param from The Solana-like address in bytes32 of the derived entity
    /// @param amount The amount to be transferred out from the derived entity
    /// @custom:getter balanceOf
    function claim(bytes32 from, uint64 amount) external returns (bool) {
        return claimTo(from, msg.sender, amount);
    }

    /// @notice Initiating transferWithSeed instuction on Solana. Before calling this method the derived address has to approve the method caller ( very similar to ERC20's transferFrom method )
    /// @param from The Solana-like address in bytes32 of the derived entity
    /// @param from The Ethereum-like address of the claimer
    /// @param amount The amount to be transferred out from the derived entity
    /// @custom:getter balanceOf
    function claimTo(bytes32 from, address to, uint64 amount) public returns (bool) {
        bytes32 toSolana = solanaAccount(to);

        if (SPL_TOKEN.isSystemAccount(toSolana)) {
            SPL_TOKEN.initializeAccount(_salt(to), tokenMint);
        }

        SPL_TOKEN.transferWithSeed(_salt(msg.sender), from, toSolana, amount);
        emit Transfer(address(0), to, amount);
        return true;
    }

    /// @notice Internal method to keep records inside the _allowances mapping
    function _approve(address owner, address spender, uint256 amount) internal {
        if (owner == address(0) || spender == address(0)) revert EmptyAddress();

        _allowances[owner][spender] = amount;
        emit Approval(owner, spender, amount);
    }

    /// @notice Internal method to update the _allowances mapping on spending
    function _spendAllowance(address owner, address spender, uint256 amount) internal {
        uint256 currentAllowance = allowance(owner, spender);
        if (currentAllowance != type(uint256).max) {
            if (currentAllowance < amount) revert InvalidAllowance();
            _approve(owner, spender, currentAllowance - amount);
        }
    }

    /// @notice Internal method to burn amounts of the SPLToken on Solana
    function _burn(address from, uint256 amount) internal {
        if (from == address(0)) revert EmptyAddress();
        if (amount > type(uint64).max) revert AmountExceedsUint64();

        bytes32 fromSolana = solanaAccount(from);
        if (SPL_TOKEN.getAccount(fromSolana).amount < amount) revert AmountExceedsBalance();
        SPL_TOKEN.burn(tokenMint, fromSolana, uint64(amount));

        emit Transfer(from, address(0), amount);
    }

    /// @notice Internal method to transfer amounts of the SPLToken on Solana
    function _transfer(address from, address to, uint256 amount) internal {
        if (from == address(0) || to == address(0)) revert EmptyAddress();

        bytes32 fromSolana = solanaAccount(from);
        bytes32 toSolana = solanaAccount(to);

        if (amount > type(uint64).max) revert AmountExceedsUint64();
        if (SPL_TOKEN.getAccount(fromSolana).amount < amount) revert AmountExceedsBalance();

        if (SPL_TOKEN.isSystemAccount(toSolana)) {
            SPL_TOKEN.initializeAccount(_salt(to), tokenMint);
        }

        SPL_TOKEN.transfer(fromSolana, toSolana, uint64(amount));
        emit Transfer(from, to, amount);
    }
}