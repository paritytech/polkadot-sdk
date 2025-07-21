// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.2 <0.9.0;

/// @title Call Data Size Contract
/// @notice Returns the call data size back to the caller.
contract CallDataSize {
    
    /// @notice Deploy function (empty implementation)
    constructor() {}
    
    /// @notice Main fallback function that returns the call data size
    fallback() external payable {
        // Return the size of the call data as little-endian bytes (8 bytes for u64)
        uint64 size = uint64(msg.data.length);
        
        // Convert to little-endian format for 8 bytes (u64)
        bytes memory result = new bytes(8);
        for (uint i = 0; i < 8; i++) {
            result[i] = bytes1(uint8(size >> (i * 8)));
        }
        
        assembly {
            return(add(result, 0x20), 0x08)
        }
    }
}