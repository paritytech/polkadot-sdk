// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ChainId {
    constructor() {
        // Call the function during deployment as well
        call();
    }
    
    function call() public view returns (uint256) {
        return block.chainid;
    }
}