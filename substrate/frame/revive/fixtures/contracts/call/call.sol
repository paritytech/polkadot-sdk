// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Call Contract
/// @notice This calls another contract as passed as its account id.
contract Call {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main call function that calls another contract
    function call() public {
        // Input format: [callee_input: 4 bytes][callee_addr: 20 bytes]
        require(msg.data.length >= 4 + 20, "Invalid input length");
        
        // Extract callee input (first 4 bytes after function selector)
        bytes memory calleeInput = new bytes(4);
        assembly {
            calldatacopy(add(calleeInput, 0x20), 4, 4)
        }
        
        // Extract callee address (next 20 bytes)
        address calleeAddr;
        assembly {
            calleeAddr := shr(96, calldataload(8))
        }
        
        // Call the callee
        (bool success, ) = calleeAddr.call(calleeInput);
        require(success, "Call to callee failed");
    }
}