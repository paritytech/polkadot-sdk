// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CallAndReturn {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: callee_addr (20 bytes), value (8 bytes), callee_input (rest)
        if (msg.data.length < 28) {
            assembly { invalid() }
        }
        
        address callee_addr = address(bytes20(msg.data[0:20]));
        uint64 value = uint64(bytes8(msg.data[20:28]));
        bytes memory callee_input = msg.data[28:];
        
        // Call the callee
        (bool success, bytes memory output) = callee_addr.call{value: value}(callee_input);
        
        if (success) {
            // Return the output normally
            assembly {
                return(add(output, 0x20), mload(output))
            }
        } else {
            // Return with revert flag
            assembly {
                revert(add(output, 0x20), mload(output))
            }
        }
    }
}