// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract DelegateCallLib {
    constructor() {
        // Empty constructor
    }
    
    function call() external payable {
        // Set a value in storage at key 1
        bytes32 key = bytes32(uint256(1));
        bytes32 value = bytes32(uint256(1));
        
        assembly {
            sstore(key, value)
        }
        
        // Assert that msg.value is equal to 1337
        require(msg.value == 1337, "Value transferred should be 1337");
        
        // Assert that msg.sender is ALICE (0x0101010101010101010101010101010101010101)
        address expectedCaller = address(0x0101010101010101010101010101010101010101);
        require(msg.sender == expectedCaller, "Caller should be ALICE");
    }
}