// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract MultiStore {
    constructor() {
        // Empty constructor
    }
    
    function call(uint32 size1, uint32 size2) external {
        require(size1 <= 512, "Size1 too large");
        require(size2 <= 512, "Size2 too large");
        
        // Create buffer data (zeros)
        bytes memory buffer1 = new bytes(size1);
        bytes memory buffer2 = new bytes(size2);
        
        // Set storage at two different keys
        // Key 1: [1, 1, 1, ...] (32 bytes of 1s)
        bytes32 key1 = bytes32(uint256(0x0101010101010101010101010101010101010101010101010101010101010101));
        // Key 2: [2, 2, 2, ...] (32 bytes of 2s)
        bytes32 key2 = bytes32(uint256(0x0202020202020202020202020202020202020202020202020202020202020202));
        
        // Store the data at the two keys
        // In Solidity, storage is key-value with 32-byte slots
        // We'll store the first 32 bytes of each buffer
        bytes32 value1;
        bytes32 value2;
        
        if (size1 > 0) {
            assembly {
                value1 := mload(add(buffer1, 0x20))
            }
            assembly {
                sstore(key1, value1)
            }
        }
        
        if (size2 > 0) {
            assembly {
                value2 := mload(add(buffer2, 0x20))
            }
            assembly {
                sstore(key2, value2)
            }
        }
    }
}