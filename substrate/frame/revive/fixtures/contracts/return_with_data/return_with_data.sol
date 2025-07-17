// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ReturnWithData {
    constructor() {
        // Call during deployment as well
        call();
    }
    
    function call() public {
        // Read the input data
        bytes calldata inputData = msg.data;
        
        if (inputData.length < 8) {
            return; // Need at least 4 bytes selector + 4 bytes exit status
        }
        
        // The output is everything after the first 8 bytes (4 selector + 4 exit status)
        bytes memory output;
        if (inputData.length > 8) {
            output = inputData[8:];
        }
        
        // Simulate some storage operation for PoV consumption
        // In Solidity, we can't directly clear storage of empty key like in Rust
        // but we can perform a storage operation
        assembly {
            let dummy := sload(0)
            sstore(0, 0)
        }
        
        // Return the data (exit status is handled by the return mechanism)
        assembly {
            return(add(output, 0x20), mload(output))
        }
    }
}