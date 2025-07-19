// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Transient Storage Contract
/// @notice This contract tests the transient storage APIs.
contract TransientStorage {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that tests various transient storage operations
    fallback() external payable {
        bytes32 key = bytes32(0x0101010101010101010101010101010101010101010101010101010101010101);
        bytes4 value1 = bytes4(0x01010101);
        bytes memory value2 = hex"0202020202";
        bytes memory value3 = hex"030303030303";
        
        // Set transient storage and check initial state
        uint256 existing = setTransientStorage(key, abi.encode(value1));
        assembly {
            if existing {
                invalid()
            }
        }
        
        uint256 length = containsTransientStorage(key);
        assembly {
            if iszero(eq(length, 4)) {
                invalid()
            }
        }
        
        bytes memory val = getTransientStorage(key);
        assembly {
            if iszero(eq(mload(val), 4)) {
                invalid()
            }
            if iszero(eq(mload(add(val, 0x20)), 0x0101010100000000000000000000000000000000000000000000000000000000)) {
                invalid()
            }
        }
        
        // Set transient storage with existing value
        existing = setTransientStorage(key, value2);
        assembly {
            if iszero(eq(existing, 4)) {
                invalid()
            }
        }
        
        val = getTransientStorage(key);
        assembly {
            if iszero(eq(mload(val), 5)) {
                invalid()
            }
            if iszero(eq(mload(add(val, 0x20)), 0x0202020202000000000000000000000000000000000000000000000000000000)) {
                invalid()
            }
        }
        
        // Clear transient storage
        uint256 clearedLength = clearTransientStorage(key);
        assembly {
            if iszero(eq(clearedLength, 5)) {
                invalid()
            }
        }
        
        length = containsTransientStorage(key);
        assembly {
            if length {
                invalid()
            }
        }
        
        // Set transient storage after clear
        existing = setTransientStorage(key, value3);
        assembly {
            if existing {
                invalid()
            }
        }
        
        val = takeTransientStorage(key);
        assembly {
            if iszero(eq(mload(val), 6)) {
                invalid()
            }
            if iszero(eq(mload(add(val, 0x20)), 0x0303030303030000000000000000000000000000000000000000000000000000)) {
                invalid()
            }
        }
    }
    
    /// @notice Set transient storage value and return previous value length
    function setTransientStorage(bytes32 key, bytes memory value) internal returns (uint256) {
        uint256 previousLength = containsTransientStorage(key);
        
        // Call set_storage syscall (0x1006) with TRANSIENT flag (1)
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 1) // TRANSIENT flag
            mstore(add(ptr, 0x20), key) // key (32 bytes)
            
            // Copy value data
            let valueLength := mload(value)
            let valuePtr := add(value, 0x20)
            for { let i := 0 } lt(i, valueLength) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x40), i), mload(add(valuePtr, i)))
            }
            
            let result := call(gas(), 0x1006, 0, ptr, add(0x40, valueLength), 0, 0)
        }
        
        return previousLength;
    }
    
    /// @notice Get transient storage value
    function getTransientStorage(bytes32 key) internal returns (bytes memory) {
        uint256 length = containsTransientStorage(key);
        if (length == 0) {
            return new bytes(0);
        }
        
        bytes memory value = new bytes(length);
        
        // Call get_storage syscall (0x1005) with TRANSIENT flag (1)
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 1) // TRANSIENT flag
            mstore(add(ptr, 0x20), key) // key (32 bytes)
            
            let result := call(gas(), 0x1005, 0, ptr, 0x40, add(value, 0x20), length)
        }
        
        return value;
    }
    
    /// @notice Check if transient storage contains a value and return its length
    function containsTransientStorage(bytes32 key) internal returns (uint256) {
        uint256 length;
        
        // Call contains_storage syscall (0x1008) with TRANSIENT flag (1)
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 1) // TRANSIENT flag
            mstore(add(ptr, 0x20), key) // key (32 bytes)
            
            let result := call(gas(), 0x1008, 0, ptr, 0x40, ptr, 0x20)
            if result {
                length := mload(ptr)
            }
        }
        
        return length;
    }
    
    /// @notice Clear transient storage value
    function clearTransientStorage(bytes32 key) internal returns (uint256) {
        uint256 length = containsTransientStorage(key);
        
        if (length > 0) {
            // Call clear_storage syscall (0x1007) with TRANSIENT flag (1)
            assembly {
                let ptr := mload(0x40)
                mstore(ptr, 1) // TRANSIENT flag
                mstore(add(ptr, 0x20), key) // key (32 bytes)
                
                let result := call(gas(), 0x1007, 0, ptr, 0x40, 0, 0)
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