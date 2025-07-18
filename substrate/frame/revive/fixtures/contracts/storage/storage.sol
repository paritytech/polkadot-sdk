// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Storage Contract
/// @notice Tests the storage APIs. It sets and clears storage values using different storage operations.
contract Storage {
    mapping(bytes32 => bytes) private storage_;
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that tests various storage operations
    fallback() external payable {
        bytes32 key = bytes32(0x0101010101010101010101010101010101010101010101010101010101010101);
        bytes memory value1 = hex"01010101";
        bytes memory value2 = hex"02020202";
        bytes memory value3 = hex"03030303";
        
        // Set storage and check contains
        setStorage(key, value1);
        require(containsStorage(key) == value1.length, "Storage should contain value1");
        bytes memory val = getStorage(key);
        require(keccak256(val) == keccak256(value1), "Retrieved value should equal value1");
        
        // Set storage with existing value
        uint256 existing = setStorage(key, value2);
        require(existing == value1.length, "Should return previous value length");
        val = getStorage(key);
        require(keccak256(val) == keccak256(value2), "Retrieved value should equal value2");
        
        // Clear storage
        clearStorage(key);
        require(containsStorage(key) == 0, "Storage should be empty after clear");
        
        // Set storage after clear
        existing = setStorage(key, value3);
        require(existing == 0, "Should return 0 for previously empty storage");
        require(containsStorage(key) == value1.length, "Storage should contain value3");
        val = getStorage(key);
        require(keccak256(val) == keccak256(value3), "Retrieved value should equal value3");
        
        // Clear and set again
        clearStorage(key);
        require(containsStorage(key) == 0, "Storage should be empty after clear");
        existing = setStorage(key, value3);
        require(existing == 0, "Should return 0 for previously empty storage");
        val = takeStorage(key);
        require(keccak256(val) == keccak256(value3), "Taken value should equal value3");
    }
    
    /// @notice Set storage value and return previous value length
    function setStorage(bytes32 key, bytes memory value) internal returns (uint256) {
        uint256 previousLength = storage_[key].length;
        storage_[key] = value;
        return previousLength;
    }
    
    /// @notice Get storage value
    function getStorage(bytes32 key) internal view returns (bytes memory) {
        return storage_[key];
    }
    
    /// @notice Check if storage contains a value and return its length
    function containsStorage(bytes32 key) internal view returns (uint256) {
        return storage_[key].length;
    }
    
    /// @notice Clear storage value
    function clearStorage(bytes32 key) internal {
        delete storage_[key];
    }
    
    /// @notice Take storage value (get and clear)
    function takeStorage(bytes32 key) internal returns (bytes memory) {
        bytes memory value = storage_[key];
        delete storage_[key];
        return value;
    }
}