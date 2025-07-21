// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract ClearStorageOnZeroValue {
    
    function deploy() external {
        // Empty deploy function
    }
    
    function testStorageOperations(uint32 flags) internal {
        bytes32 key = bytes32(uint256(0x0101010101010101010101010101010101010101010101010101010101010101));
        bytes32 valueA = bytes32(uint256(0x0404040404040404040404040404040404040404040404040404040404040404));
        bytes32 zero = bytes32(0);
        bytes32 smallValue = bytes32(uint256(0x0506070000000000000000000000000000000000000000000000000000000000));
        
        // Clear storage
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            let result := call(gas(), 0x1007, 0, ptr, 0x40, 0, 0)
        }
        
        // Check if storage contains key (should be empty)
        bool contains;
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            let result := call(gas(), 0x1008, 0, ptr, 0x40, ptr, 0x20)
            contains := mload(ptr)
        }
        if (contains) {
            assembly { invalid() }
        }
        
        // Set storage with valueA
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            mstore(add(ptr, 0x40), valueA)
            let result := call(gas(), 0x1006, 0, ptr, 0x60, 0, 0)
        }
        
        // Get storage and verify it's valueA
        bytes32 stored;
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            let result := call(gas(), 0x1005, 0, ptr, 0x40, ptr, 0x20)
            stored := mload(ptr)
        }
        if (stored != valueA) {
            assembly { invalid() }
        }
        
        // Set storage with zero value (should clear it)
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            mstore(add(ptr, 0x40), zero)
            let result := call(gas(), 0x1006, 0, ptr, 0x60, 0, 0)
        }
        
        // Get storage and verify it's zero
        bytes32 cleared;
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            let result := call(gas(), 0x1005, 0, ptr, 0x40, ptr, 0x20)
            cleared := mload(ptr)
        }
        if (cleared != zero) {
            assembly { invalid() }
        }
        
        // Check if storage contains key (should be empty again)
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            let result := call(gas(), 0x1008, 0, ptr, 0x40, ptr, 0x20)
            contains := mload(ptr)
        }
        if (contains) {
            assembly { invalid() }
        }
        
        // Test with small value
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            mstore(add(ptr, 0x40), smallValue)
            let result := call(gas(), 0x1006, 0, ptr, 0x60, 0, 0)
        }
        
        // Get and verify small value
        bytes32 retrieved;
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            let result := call(gas(), 0x1005, 0, ptr, 0x40, ptr, 0x20)
            retrieved := mload(ptr)
        }
        if (retrieved != smallValue) {
            assembly { invalid() }
        }
        
        // Clean up
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, flags)
            mstore(add(ptr, 0x20), key)
            let result := call(gas(), 0x1007, 0, ptr, 0x40, 0, 0)
        }
    }
    
    fallback() external payable {
        // Test with regular storage (flags = 0)
        testStorageOperations(0);
        
        // Test with transient storage (flags = 1)
        testStorageOperations(1);
    }
}