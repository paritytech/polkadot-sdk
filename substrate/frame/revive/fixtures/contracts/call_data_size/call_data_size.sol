// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Call Data Size Contract
/// @notice Returns the call data size back to the caller.
contract CallDataSize {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main call function that returns the call data size
    function call() public pure returns (uint32) {
        // Return the size of the call data
        return uint32(msg.data.length);
    }
}