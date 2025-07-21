// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CallDataLoad {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        assembly {
            // Load first 32 bytes of call data
            let buf := calldataload(0)
            
            // Get buf[31] (the 32nd byte) as offset
            // Extract the least significant byte
            let offset := and(buf, 0xff)
            
            // Load 32 bytes from the calculated offset  
            buf := calldataload(offset)
            
            // Return the result
            mstore(0x00, buf)
            return(0x00, 0x20)
        }
    }
}