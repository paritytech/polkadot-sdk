// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CodeHash {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: address (20 bytes), expected_code_hash (32 bytes)
        if (msg.data.length < 52) {
            revert("Invalid input length");
        }
        
        address addr;
        bytes32 expected_code_hash;
        
        assembly {
            addr := shr(96, calldataload(0))
            expected_code_hash := calldataload(20)
        }
        
        // Get code hash and compare
        bytes32 actual_code_hash = addr.codehash;
        if (actual_code_hash != expected_code_hash) {
            revert("Code hash mismatch");
        }
    }
}