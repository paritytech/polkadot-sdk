// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BaseFee {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        uint256 baseFee = block.basefee;
        // Return as 32-byte array
        assembly {
            mstore(0x00, baseFee)
            return(0x00, 0x20)
        }
    }
}