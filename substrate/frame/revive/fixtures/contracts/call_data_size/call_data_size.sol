// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Call Data Size Contract
/// @notice Returns the call data size back to the caller.
contract CallDataSize {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that returns the call data size
    fallback() external payable {
        // Return the size of the call data as little-endian bytes
        uint32 size = uint32(msg.data.length);
        assembly {
            mstore(0x00, size)
            return(0x00, 0x04)
        }
    }
}