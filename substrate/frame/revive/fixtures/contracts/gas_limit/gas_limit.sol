// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract GasLimit {
    constructor() {
        // Empty constructor
    }
    
    function call() external view returns (uint64) {
        return uint64(gasleft());
    }
}