// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BlockAuthor {
    constructor() {
        // Empty constructor
    }
    
    fallback() external {
        // Extract address from call data (first 20 bytes)
        require(msg.data.length >= 20, "Not enough data");
        
        address expected;
        assembly {
            expected := shr(96, calldataload(0))
        }
        
        address actual = block.coinbase;
        require(actual == expected, "Block author mismatch");
    }
}