// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract EventAndReturnOnDeploy {
    event TestEvent(bytes indexed topic, bytes data);
    
    constructor() {
        bytes memory buffer = hex"01020304";
        bytes32 topic = bytes32(uint256(42));
        emit TestEvent(abi.encodePacked(topic), buffer);
        
        // Return the buffer data
        assembly {
            return(add(buffer, 0x20), 0x04)
        }
    }
    
    fallback() external payable {
        revert("Should not be called");
    }
}