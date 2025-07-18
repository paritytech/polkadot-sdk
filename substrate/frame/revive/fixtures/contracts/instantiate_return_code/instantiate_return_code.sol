// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract InstantiateReturnCode {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: buffer (36 bytes)
        require(msg.data.length >= 36, "Invalid input length");
        
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
            for { let i := 0 } lt(i, buffer_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x50), i), mload(add(buffer_ptr, i)))
            }
            
            // salt (32 bytes of 0)
            mstore(add(add(ptr, 0x50), buffer_len), 0)
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, add(0x70, buffer_len), 0, 0)
            
            switch result
            case 0 {
                // For now, return a generic error code
                err_code := 1
            }
            default {
                err_code := 0
            }
        }
    }
}