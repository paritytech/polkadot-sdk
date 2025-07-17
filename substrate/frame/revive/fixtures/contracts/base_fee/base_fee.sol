// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BaseFee {
    constructor() {
        // Empty constructor
    }
    
    function call() external view returns (uint256) {
        return block.basefee;
    }
}