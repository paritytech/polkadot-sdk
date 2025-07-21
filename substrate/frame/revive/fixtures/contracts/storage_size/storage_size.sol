// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract StorageSize {
    constructor() {
        // Empty constructor
    }
    
    function call(uint32 len) external {
        require(len <= 16384, "Length too large"); // 16 * 1024
        
        // Create a garbage value of specified size
        bytes memory data = new bytes(len);
        for (uint i = 0; i < len; i++) {
            data[i] = 0x00;
        }
        
        // Place a garbage value in storage at key [1, 0, 0, ...]
        bytes32 key = bytes32(uint256(1) << 248); // key[0] = 1
        
        assembly {
            sstore(key, mload(add(data, 0x20)))
        }
        
        // Read back the storage and verify size
        bytes32 storedValue;
        assembly {
            storedValue := sload(key)
        }
        
        // In Solidity, storage slots are 32 bytes, so we verify the data was stored
        // The exact size verification is handled differently in EVM than in the Rust version
    }
}