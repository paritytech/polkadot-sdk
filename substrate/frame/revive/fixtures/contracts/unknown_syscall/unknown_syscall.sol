// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract UnknownSyscall {
    constructor() {
        // Empty constructor
    }
    
    function callNever() external {
        // Make sure it is not optimized away
        // Call to a non-existent syscall
        assembly {
            let result := call(gas(), 0xFFFE, 0, 0, 0, 0, 0)
        }
    }
    
    fallback() external payable {
        // Empty fallback function
    }
}