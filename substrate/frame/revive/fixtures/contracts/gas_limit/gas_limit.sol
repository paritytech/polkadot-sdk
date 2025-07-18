// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract GasLimit {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        uint64 gasLimit = uint64(gasleft());
        // Return as little-endian 8-byte array
        assembly {
            mstore(0x00, gasLimit)
            return(0x00, 0x08)
        }
    }
}