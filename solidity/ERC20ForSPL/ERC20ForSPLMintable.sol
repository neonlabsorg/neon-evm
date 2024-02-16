// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import "./openzeppelin-fork/contracts-upgradeable/proxy/utils/Initializable.sol";
import "./openzeppelin-fork/contracts-upgradeable/access/OwnableUpgradeable.sol";
import './ERC20ForSPLBackbone.sol';


/// @title ERC20ForSPLMintable
/// @author https://twitter.com/mnedelchev_
/// @notice This contract serves as an interface to the deployed SPLToken on Solana. Through this interface, Ethereum-like address on Neon EVM chain can apply changes on SPLToken account on Solana.
/// @dev This contract is being used as a BeaconProxy implementation. The Beacon is defined and inherited from ERC20ForSPLBackbone.sol at storage slot 0.
/// @custom:oz-upgrades-unsafe-allow constructor
contract ERC20ForSPLMintable is ERC20ForSPLBackbone, Initializable, OwnableUpgradeable {
    error InvalidDecimals();

    /// @notice Disabling the initializers to prevent the implementation getting hijacked
    constructor() {
        _disableInitializers();
    }

    /// @notice Method used by the OpenZeppelin's UUPS lib that mimics the functionality of a constructor
    /// @param _name The name of the SPLToken
    /// @param _symbol The symbol of the SPLToken
    /// @param _decimals The decimals of the SPLToken. This value cannot be bigger than 9, because of Solana's maximum value limit of uint64
    /// @param _owner The owner of the ERC20ForSPLMintable contract which has the permissions to mint new tokens
    function initialize(
        string memory _name,
        string memory _symbol,
        uint8 _decimals,
        address _owner
    ) public initializer {
        __Ownable_init(_owner);

        if (_decimals > 9) revert InvalidDecimals();
        
        bytes32 _tokenMint = _initialize(_name, _symbol, _decimals);
        if (!SPL_TOKEN.getMint(_tokenMint).isInitialized) revert InvalidTokenMint();
        if (!METAPLEX.isInitialized(_tokenMint)) revert MissingMetaplex();

        tokenMint = _tokenMint;
    }

    /// @notice Returns the Solana address of the Token Mint
    function findMintAccount() public pure returns (bytes32) {
        return SPL_TOKEN.findAccount(bytes32(0));
    }

    /// @notice Mint new SPLToken directly on Solana chain
    /// @custom:getter balanceOf
    function mint(address to, uint256 amount) public onlyOwner {
        if (to == address(0)) revert EmptyAddress();
        if (totalSupply() + amount > type(uint64).max) revert AmountExceedsUint64();

        bytes32 toSolana = solanaAccount(to);
        if (SPL_TOKEN.isSystemAccount(toSolana)) {
            SPL_TOKEN.initializeAccount(_salt(to), tokenMint);
        }

        SPL_TOKEN.mintTo(tokenMint, toSolana, uint64(amount));
        emit Transfer(address(0), to, amount);
    }

    /// @notice Internal method which deploys the Token Mint on Solana and setups the metadata ( name & symbol ) to the Metaplex protocol on Solana
    function _initialize(
        string memory _name,
        string memory _symbol,
        uint8 _decimals
    ) private returns (bytes32) {
        bytes32 mintAddress = SPL_TOKEN.initializeMint(bytes32(0), _decimals);
        METAPLEX.createMetadata(mintAddress, _name, _symbol, "");
        return mintAddress;
    }
}