// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract BlockHash {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Extract block number and expected block hash from calldata
        require(msg.data.length >= 64, "Insufficient calldata");
        
        bytes32 blockNumber;
        bytes32 expectedBlockHash;
        
        assembly {
            blockNumber := calldataload(0)
            expectedBlockHash := calldataload(32)
        }
        
        bytes32 actualBlockHash = blockhash(uint256(blockNumber));
        require(actualBlockHash == expectedBlockHash, "Block hash mismatch");
    }
}