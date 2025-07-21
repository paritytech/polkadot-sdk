// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract InstantiateReturnCode {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: buffer (36 bytes)
        if (msg.data.length < 36) {
            assembly {
                invalid()
            }
        }
        
        bytes memory buffer = msg.data[0:36];
        
        // Instantiate the contract
        uint32 err_code = instantiateAndGetErrorCode(buffer);
        
        // Return the error code
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, err_code)
            return(ptr, 4)
        }
    }
    
    function instantiateAndGetErrorCode(bytes memory buffer) internal returns (uint32 err_code) {
        assembly {
            let ptr := mload(0x40)
            
            // ref_time_limit: u64::MAX
            mstore(ptr, 0xffffffffffffffff)
            
            // proof_size_limit: u64::MAX  
            mstore(add(ptr, 0x08), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x10), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (10_000 as u256)
            mstore(add(ptr, 0x30), 10000)
            
            // buffer data (36 bytes) 
            let buffer_ptr := add(buffer, 0x20)
            let buffer_len := mload(buffer)
            
            // Copy buffer data in 32-byte chunks
            for { let i := 0 } lt(i, buffer_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x50), i), mload(add(buffer_ptr, i)))
            }
            
            // salt (32 bytes of 0)
            mstore(add(add(ptr, 0x50), buffer_len), 0)
            
            // Prepare output buffer for the instantiate call
            let output_ptr := add(ptr, add(0x70, buffer_len))
            
            // Call instantiate syscall (0x3001)
            // The syscall returns success/failure and writes error code to output buffer
            let success := call(gas(), 0x3001, 0, ptr, add(0x70, buffer_len), output_ptr, 4)
            
            switch success
            case 0 {
                // Call failed - read the error code from output buffer if available
                err_code := mload(output_ptr)
                // If no error code was written, use a default error
                if iszero(err_code) {
                    err_code := 1
                }
            }
            default {
                // Call succeeded - return 0 for success
                err_code := 0
            }
        }
    }
}