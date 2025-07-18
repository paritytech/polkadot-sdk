// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Call Contract
/// @notice This calls another contract as passed as its account id.
contract Call {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that calls another contract
    fallback() external payable {
        // Input format: [callee_input: 4 bytes][callee_addr: 20 bytes]
        require(msg.data.length >= 24, "Invalid input length");
        
        // Extract callee input (first 4 bytes)
        bytes memory calleeInput = new bytes(4);
        assembly {
            calldatacopy(add(calleeInput, 0x20), 0, 4)
        }
        
        // Extract callee address (next 20 bytes)
        address calleeAddr;
        assembly {
            calleeAddr := shr(96, calldataload(4))
        }
        
        // Call the callee
        (bool success, ) = calleeAddr.call(calleeInput);
        require(success, "Call to callee failed");
    }
}