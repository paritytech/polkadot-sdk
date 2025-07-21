// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract TracingCallee {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: id (4 bytes)
        if (msg.data.length < 4) {
            assembly {
                invalid()
            }
        }
        
        uint32 id = uint32(bytes4(msg.data[0:4]));
        
        if (id == 2) {
            // Revert with message "This function always fails"
            revert("This function always fails");
        } else if (id == 1) {
            // Panic
            assembly {
                invalid()
            }
        } else {
            // Return id as little-endian bytes
            assembly {
                mstore(0x00, id)
                return(0x00, 0x04)
            }
        }
    }
}