// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract EventSize {
    event TestEvent(bytes data);
    
    constructor() {
        // Empty constructor
    }
    
    function call(uint32 len) external {
        // Create a buffer with the specified length
        bytes memory data = new bytes(len);
        // Fill with zeros (default initialization)
        
        emit TestEvent(data);
    }
}