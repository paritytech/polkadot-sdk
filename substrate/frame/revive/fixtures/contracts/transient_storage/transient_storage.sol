// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Transient Storage Contract
/// @notice This contract tests the transient storage APIs.
contract TransientStorage {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main call function that tests various transient storage operations
    function call() public {
        bytes32 key = bytes32(0x0101010101010101010101010101010101010101010101010101010101010101);
        bytes memory value1 = hex"01010101";
        bytes memory value2 = hex"0202020202";
        bytes memory value3 = hex"030303030303";
        
        // Set transient storage and check initial state
        uint256 existing = setTransientStorage(key, value1);
        require(existing == 0, "Initial set should return 0");
        require(containsTransientStorage(key) == value1.length, "Storage should contain value1");
        bytes memory val = getTransientStorage(key);
        require(keccak256(val) == keccak256(value1), "Retrieved value should equal value1");
        
        // Set transient storage with existing value
        existing = setTransientStorage(key, value2);
        require(existing == value1.length, "Should return previous value length");
        val = getTransientStorage(key);
        require(keccak256(val) == keccak256(value2), "Retrieved value should equal value2");
        
        // Clear transient storage
        uint256 clearedLength = clearTransientStorage(key);
        require(clearedLength == value2.length, "Should return cleared value length");
        require(containsTransientStorage(key) == 0, "Storage should be empty after clear");
        
        // Set transient storage after clear
        existing = setTransientStorage(key, value3);
        require(existing == 0, "Should return 0 for previously empty storage");
        val = takeTransientStorage(key);
        require(keccak256(val) == keccak256(value3), "Taken value should equal value3");
    }
    
    /// @notice Set transient storage value and return previous value length
    function setTransientStorage(bytes32 key, bytes memory value) internal returns (uint256) {
        uint256 previousLength = containsTransientStorage(key);
        
        // Store the length first
        bytes32 lengthKey = keccak256(abi.encodePacked(key, "length"));
        assembly {
            tstore(lengthKey, mload(add(value, 0x20)))
        }
        
        // Store the value in chunks of 32 bytes
        uint256 valueLength = value.length;
        for (uint256 i = 0; i < valueLength; i += 32) {
            bytes32 chunkKey = keccak256(abi.encodePacked(key, "chunk", i));
            bytes32 chunk;
            assembly {
                chunk := mload(add(add(value, 0x20), i))
            }
            assembly {
                tstore(chunkKey, chunk)
            }
        }
        
        return previousLength;
    }
    
    /// @notice Get transient storage value
    function getTransientStorage(bytes32 key) internal view returns (bytes memory) {
        uint256 length = containsTransientStorage(key);
        if (length == 0) {
            return new bytes(0);
        }
        
        bytes memory value = new bytes(length);
        for (uint256 i = 0; i < length; i += 32) {
            bytes32 chunkKey = keccak256(abi.encodePacked(key, "chunk", i));
            bytes32 chunk;
            assembly {
                chunk := tload(chunkKey)
            }
            assembly {
                mstore(add(add(value, 0x20), i), chunk)
            }
        }
        
        return value;
    }
    
    /// @notice Check if transient storage contains a value and return its length
    function containsTransientStorage(bytes32 key) internal view returns (uint256) {
        bytes32 lengthKey = keccak256(abi.encodePacked(key, "length"));
        uint256 length;
        assembly {
            length := tload(lengthKey)
        }
        return length;
    }
    
    /// @notice Clear transient storage value
    function clearTransientStorage(bytes32 key) internal returns (uint256) {
        uint256 length = containsTransientStorage(key);
        
        if (length > 0) {
            // Clear the length
            bytes32 lengthKey = keccak256(abi.encodePacked(key, "length"));
            assembly {
                tstore(lengthKey, 0)
            }
            
            // Clear the value chunks
            for (uint256 i = 0; i < length; i += 32) {
                bytes32 chunkKey = keccak256(abi.encodePacked(key, "chunk", i));
                assembly {
                    tstore(chunkKey, 0)
                }
            }
        }
        
        return length;
    }
    
    /// @notice Take transient storage value (get and clear)
    function takeTransientStorage(bytes32 key) internal returns (bytes memory) {
        bytes memory value = getTransientStorage(key);
        clearTransientStorage(key);
        return value;
    }
}