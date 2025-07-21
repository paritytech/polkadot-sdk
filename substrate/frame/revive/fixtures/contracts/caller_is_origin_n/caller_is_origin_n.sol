// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CallerIsOriginN {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        uint32 n = abi.decode(msg.data, (uint32));
        
        for (uint32 i = 0; i < n; i++) {
            assembly {
                let result := call(gas(), 0x01, 0, 0, 0, 0, 0)
            }
        }
    }
}