// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Delegate Call Contract
/// @notice Tests delegate call functionality with storage operations.
contract DelegateCall {
    mapping(bytes32 => bytes32) private storage_;
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that tests delegate call with storage
    fallback() external payable {
        // Input format: [address: 20 bytes][ref_time: 8 bytes][proof_size: 8 bytes]
        require(msg.data.length >= 20 + 8 + 8, "Invalid input length");
        
        // Extract delegate call target address
        address target;
        assembly {
            target := shr(96, calldataload(0))
        }
        
        // Set up storage key and value
        bytes32 key = bytes32(uint256(1));
        bytes32 value = bytes32(uint256(2));
        
        // Set initial storage value
        storage_[key] = value;
        
        // Verify initial storage value
        require(storage_[key] == value, "Initial storage value should be 2");
        
        // Perform delegate call
        (bool success, ) = target.delegatecall("");
        require(success, "Delegate call failed");
        
        // Check that storage was modified by the delegate call
        require(storage_[key] == bytes32(uint256(1)), "Storage should be modified to 1 by delegate call");
    }
    
    /// @notice Get storage value for testing
    function getStorage(bytes32 key) public view returns (bytes32) {
        return storage_[key];
    }
    
    /// @notice Set storage value for testing
    function setStorage(bytes32 key, bytes32 value) public {
        storage_[key] = value;
    }
}