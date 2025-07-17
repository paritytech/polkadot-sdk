// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BlockAuthor {
    constructor() {
        // Empty constructor
    }
    
    function call(address expected) external view {
        address actual = block.coinbase;
        require(actual == expected, "Block author mismatch");
    }
}