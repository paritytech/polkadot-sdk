// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract UnstableInterface {
    constructor() {
        // Empty constructor
    }
    
    function callNever() external {
        // Make sure it is not optimized away
        // Call to unstable interface (set_code_hash syscall)
        assembly {
            let result := call(gas(), 0x2002, 0, 0, 0, 0, 0)
        }
    }
    
    fallback() external payable {
        // Empty fallback function
    }
}