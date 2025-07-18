// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract RunOutOfGas {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Infinite loop to consume all gas
        while (true) {
            // Empty loop body - this will consume gas until it runs out
        }
    }
}