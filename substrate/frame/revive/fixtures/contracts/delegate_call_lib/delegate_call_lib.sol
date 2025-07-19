// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract DelegateCallLib {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Set a value in storage at key 1
        bytes32 key = bytes32(uint256(1));
        bytes32 value = bytes32(uint256(1));
        
        assembly {
            sstore(key, value)
        }
        
        // Assert that msg.value is equal to 1337
        if (msg.value != 1337) {
            assembly { invalid() }
        }
        
        // Assert that msg.sender is ALICE (0x0101010101010101010101010101010101010101)
        address expectedCaller = address(0x0101010101010101010101010101010101010101);
        if (msg.sender != expectedCaller) {
            assembly { invalid() }
        }
    }
}