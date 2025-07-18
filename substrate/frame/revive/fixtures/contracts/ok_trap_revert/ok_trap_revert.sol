// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract OkTrapRevert {
    
    function deploy() external {
        okTrapRevert();
    }
    
    fallback() external payable {
        okTrapRevert();
    }
    
    function okTrapRevert() internal {
        // Check if there's input data
        if (msg.data.length >= 4) {
            uint8 first_byte = uint8(msg.data[0]);
            
            if (first_byte == 1) {
                // Return with revert flag
                assembly {
                    revert(0, 0)
                }
            } else if (first_byte == 2) {
                // Panic/trap
                assembly {
                    invalid()
                }
            }
            // For other values, do nothing and return normally
        }
    }
}