// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract SelfDestruct {
    address constant DJANGO_FALLBACK = address(0x0404040404040404040404040404040404040404);
    
    function deploy() external {
        // Set immutable data during deployment
        assembly {
            // Call set_immutable_data with [1, 2, 3, 4, 5]
            let ptr := mload(0x40)
            mstore(ptr, 0x0102030405000000000000000000000000000000000000000000000000000000)
            let result := call(gas(), 0x2000, 0, ptr, 5, 0, 0)
        }
    }
    
    fallback() external payable {
        // If input data is not empty, recursively call self with empty input
        if (msg.data.length > 0) {
            address addr = address(this);
            
            // Make recursive call with empty data
            (bool success, ) = addr.call{gas: gasleft()}("");
            
            // Trap if the call failed (equivalent to unwrap() in Rust)
            if (!success) {
                assembly {
                    invalid()
                }
            }
        } else {
            // Try to terminate and give balance to django
            assembly {
                // Call terminate syscall with DJANGO_FALLBACK address
                let ptr := mload(0x40)
                mstore(ptr, 0x0404040404040404040404040404040404040404)
                let result := call(gas(), 0x2003, 0, ptr, 20, 0, 0)
            }
        }
    }
}