// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract NewSetCodeHashContract {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Return 2
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 2)
            return(ptr, 4)
        }
    }
}