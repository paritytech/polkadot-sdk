// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ChainId {
    constructor() {
        // Call the internal function during deployment as well
        _getChainId();
    }
    
    function _getChainId() internal view {
        uint256 chainId = block.chainid;
        // Return as 32-byte array
        assembly {
            mstore(0x00, chainId)
            return(0x00, 0x20)
        }
    }
    
    fallback() external payable {
        _getChainId();
    }
}