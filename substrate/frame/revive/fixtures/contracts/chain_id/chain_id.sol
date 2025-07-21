// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ChainId {
    constructor() {
        // Call the internal function during deployment as well
        _getChainId();
    }
    
    function _getChainId() internal view {
        uint256 chainId = block.chainid;
        
        // Convert to little-endian format for 32 bytes
        bytes memory result = new bytes(32);
        for (uint i = 0; i < 32; i++) {
            result[i] = bytes1(uint8(chainId >> (i * 8)));
        }
        
        assembly {
            return(add(result, 0x20), 0x20)
        }
    }
    
    fallback() external payable {
        _getChainId();
    }
}