// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract TerminateAndSendToArgument {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: beneficiary (20 bytes)
        require(msg.data.length >= 20, "Invalid input length");
        
        address beneficiary = address(bytes20(msg.data[0:20]));
        
        // Terminate and send balance to beneficiary
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, beneficiary)
            
            // Call terminate syscall (0x2003)
            let result := call(gas(), 0x2003, 0, ptr, 0x20, 0, 0)
            
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
}