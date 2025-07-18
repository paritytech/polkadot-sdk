// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CodeHash {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: address (20 bytes), expected_code_hash (32 bytes)
        require(msg.data.length >= 52, "Invalid input length");
        
        address addr = address(bytes20(msg.data[0:20]));
        bytes32 expected_code_hash = bytes32(msg.data[20:52]);
        
        // Get code hash and compare
        bytes32 actual_code_hash = addr.codehash;
        require(actual_code_hash == expected_code_hash, "Code hash mismatch");
    }
}