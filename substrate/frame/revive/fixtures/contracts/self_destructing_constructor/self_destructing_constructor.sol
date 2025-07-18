// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract SelfDestructingConstructor {
    
    function deploy() external {
        // Try to terminate during deployment - should fail
        assembly {
            let ptr := mload(0x40)
            // Set beneficiary to address(0)
            mstore(ptr, 0)
            
            // Call terminate syscall (0x2003)
            let result := call(gas(), 0x2003, 0, ptr, 20, 0, 0)
        }
    }
    
    fallback() external payable {
        // Empty fallback function
    }
}