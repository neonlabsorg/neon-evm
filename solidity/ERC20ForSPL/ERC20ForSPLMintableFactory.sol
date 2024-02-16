// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import "./openzeppelin-fork/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "./openzeppelin-fork/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "./openzeppelin-fork/contracts/proxy/beacon/BeaconProxy.sol";
import "./interfaces/IERC20ForSPLMintable.sol";
import "./ERC20ForSPLMintable.sol";


/// @title ERC20ForSPLMintableFactory
/// @author https://twitter.com/mnedelchev_
/// @notice This contract serves as a factory to deploy SPLToken on Solana together with interface contract on Neon EVM.
/// @dev This contract is built with forked OpenZeppelin's UUPS standard and it's a Beacon contract at the same time. The storage is defined in the following way:
/// @dev Storage slot 0 - taken by the forked BeaconProxy's implementation.
/// @dev Storage slot 1 - taken by the forked UUPS's implementation.
/// @dev Storage slot 2 - taken by the forked OwnableUpgradeable's owner.
/// @dev Every next slot is defined by the needs of the ERC20ForSPLMintableFactory.
/// @custom:oz-upgrades-unsafe-allow constructor
contract ERC20ForSPLMintableFactory is OwnableUpgradeable, UUPSUpgradeable {
    address private _implementation;
    address private _uupsImplementation;
    address private _owner;
    mapping(bytes32 => address) public tokensData;
    address[] public tokens;
    address public beacon;

    event TokenDeploy(bytes32 indexed tokenMint, address indexed token);
    event Upgraded(address indexed implementation);

    error BeaconInvalidImplementation(address implementation);

    /// @notice Disabling the initializers to prevent the implementation getting hijacked
    constructor() {
        _disableInitializers();
    }

    /// @notice Method used by the OpenZeppelin's BeaconProxy lib that mimics the functionality of a constructor
    /// @param implementation_ The address of the BeaconProxy initial implementation
    function initialize(address implementation_) public initializer {       
        __Ownable_init(msg.sender);
         _setImplementation(implementation_);
    }

    function _authorizeUpgrade(address) internal override onlyOwner {}

    /**
     * @dev Returns the current implementation address.
     */
    function implementation() public view virtual returns (address) {
        return _implementation;
    }

    /**
     * @dev Upgrades the beacon to a new implementation.
     *
     * Emits an {Upgraded} event.
     *
     * Requirements:
     *
     * - msg.sender must be the owner of the contract.
     * - `newImplementation` must be a contract.
     */
    function upgradeTo(address newImplementation) public virtual onlyOwner {
        _setImplementation(newImplementation);
    }

    /**
     * @dev Sets the implementation contract address for this beacon
     *
     * Requirements:
     *
     * - `newImplementation` must be a contract.
     */
    function _setImplementation(address newImplementation) private {
        if (newImplementation.code.length == 0) {
            revert BeaconInvalidImplementation(newImplementation);
        }
        _implementation = newImplementation;
        emit Upgraded(newImplementation);
    }

    /// @notice Deploys a new ERC20ForSPLMintable's BeaconProxy instance
    /// @param _name The name of the SPLToken
    /// @param _symbol The symbol of the SPLToken
    /// @param _decimals The decimals of the SPLToken. This value cannot be bigger than 9, because of Solana's maximum value limit of uint64
    function deploy(
        string memory _name,
        string memory _symbol,
        uint8 _decimals
    ) external {
        BeaconProxy token = new BeaconProxy(
            address(this),
            abi.encodeWithSelector(
                ERC20ForSPLMintable(address(0)).initialize.selector, 
                _name,
                _symbol,
                _decimals,
                msg.sender
            )
        );

        bytes32 tokenMint = IERC20ForSPLMintable(address(token)).tokenMint();
        tokensData[tokenMint] = address(token);
        tokens.push(address(token));

        emit TokenDeploy(tokenMint, address(token));
    }
}