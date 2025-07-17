// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CallDataLoad {
    constructor() {
        // Empty constructor
    }
    
    function call() external pure returns (bytes32) {
        bytes32 buf;
        
        // Load first 32 bytes of call data (including selector)
        assembly {
            buf := calldataload(0)
        }
        
        // Get the offset from the last byte of the first 32-byte word
        uint32 offset = uint32(uint8(buf[31]));
        
        // Load 32 bytes from the calculated offset
        assembly {
            buf := calldataload(offset)
        }
        
        return buf;
    }
}