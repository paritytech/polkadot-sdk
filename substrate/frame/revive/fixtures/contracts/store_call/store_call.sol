// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract StoreCall {
    constructor() {
        // Empty constructor
    }
    
    function call(uint32 len) external {
        if (len > 512) {
            assembly { invalid() }
        }
        
        // Create a buffer of zeros with specified length
        bytes memory data = new bytes(len);
        
        // Key with first byte set to 1, rest zeros
        bytes32 key = bytes32(uint256(1) << 248); // key[0] = 1
        
        // Store the data
        // In Solidity, we can only store 32-byte values, so we'll store the first 32 bytes
        bytes32 value;
        if (len > 0) {
            assembly {
                value := mload(add(data, 0x20))
            }
        }
        
        assembly {
            sstore(key, value)
        }
    }
}