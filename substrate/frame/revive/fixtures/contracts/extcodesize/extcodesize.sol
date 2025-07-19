// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ExtCodeSize {
    constructor() {
        // Empty constructor
    }
    
    function call(address target, uint64 expected) external view {
        uint256 codeSize;
        assembly {
            codeSize := extcodesize(target)
        }
        
        if (codeSize != expected) {
            assembly { invalid() }
        }
    }
}