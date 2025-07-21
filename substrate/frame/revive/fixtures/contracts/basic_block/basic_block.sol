// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BasicBlock {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Empty fallback function
    }
    
    function callNever() external {
        // This function creates a large basic block by performing many operations
        // In EVM/Solidity, we can't control basic block size as directly as in assembly
        // But we can create many sequential operations
        
        uint256 value = 42;
        
        // Perform many storage operations to create a large basic block
        // Note: This will use gas and storage, unlike the Rust version which is never called
        for (uint i = 0; i < 100; i++) {
            assembly {
                let slot := add(i, 0x1000)
                sstore(slot, value)
            }
        }
        
        revert("This function should never be called");
    }
}