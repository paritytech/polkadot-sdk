// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract GasPrice {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        uint256 gasPrice = tx.gasprice;
        // Return as little-endian 8-byte array
        assembly {
            mstore(0x00, gasPrice)
            return(0x00, 0x08)
        }
    }
}