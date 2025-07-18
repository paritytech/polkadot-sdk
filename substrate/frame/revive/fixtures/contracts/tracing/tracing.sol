// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Tracing {
    event TestEvent(bytes data);
    
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: calls_left (4 bytes), callee_addr (20 bytes)
        require(msg.data.length >= 24, "Invalid input length");
        
        uint32 calls_left = uint32(bytes4(msg.data[0:4]));
        address callee_addr = address(bytes20(msg.data[4:24]));
        
        if (calls_left == 0) {
            // Transfer some value to BOB (address 0x0202020202020202020202020202020202020202)
            address bob = address(0x0202020202020202020202020202020202020202);
            (bool success, ) = bob.call{value: 100}("");
            // Ignore success, just return
            return;
        }
        
        uint32 next_calls = calls_left - 1;
        bytes memory next_input = abi.encodePacked(next_calls, callee_addr);
        
        // Emit event before
        emit TestEvent("before");
        
        // Call the callee, ignore revert
        (bool success, ) = callee_addr.call{value: 0}(abi.encodePacked(next_calls, callee_addr));
        
        // Emit event after
        emit TestEvent("after");
        
        // Get own address and recurse
        address addr = address(this);
        (success, ) = addr.call{value: 0}(next_input);
        require(success, "Recursive call failed");
    }
}