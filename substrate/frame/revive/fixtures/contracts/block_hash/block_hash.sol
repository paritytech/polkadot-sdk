// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BlockHash {
    constructor() {
        // Empty constructor
    }
    
    function call(uint256 blockNumber, bytes32 expectedBlockHash) external view {
        bytes32 actualBlockHash = blockhash(blockNumber);
        require(actualBlockHash == expectedBlockHash, "Block hash mismatch");
    }
}