// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CreateTransientStorageAndCall {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: len (u32), input (4 bytes), callee (20 bytes)
        require(msg.data.length >= 28, "Invalid input length");
        
        uint32 len = uint32(bytes4(msg.data[0:4]));
        bytes memory input = msg.data[4:8];
        address callee = address(bytes20(msg.data[8:28]));
        
        // Buffer size is 416 bytes
        uint256 bufferSize = 416;
        
        // Calculate rounds and rest
        uint256 rounds = len / bufferSize;
        uint256 rest = len % bufferSize;
        
        // Create transient storage
        bytes memory buffer = new bytes(bufferSize);
        
        // Set storage for each round
        for (uint256 i = 0; i < rounds; i++) {
            assembly {
                let ptr := mload(0x40)
                mstore(ptr, 1) // TRANSIENT flag
                mstore(add(ptr, 0x20), i) // key as little-endian bytes
                mstore(add(ptr, 0x40), 0) // padding for key
                mstore(add(ptr, 0x60), 0) // padding for key
                mstore(add(ptr, 0x80), 0) // padding for key
                
                // Copy buffer data starting at ptr + 0xa0
                let bufferPtr := add(buffer, 0x20)
                for { let j := 0 } lt(j, bufferSize) { j := add(j, 0x20) } {
                    mstore(add(add(ptr, 0xa0), j), mload(add(bufferPtr, j)))
                }
                
                // Call set_storage syscall (0x1006)
                let result := call(gas(), 0x1006, 0, ptr, add(0xa0, bufferSize), 0, 0)
            }
        }
        
        // Set storage for the remaining bytes
        if (rest > 0) {
            assembly {
                let ptr := mload(0x40)
                mstore(ptr, 1) // TRANSIENT flag
                mstore(add(ptr, 0x20), 0xffffffff) // u32::MAX as key
                mstore(add(ptr, 0x40), 0) // padding for key
                mstore(add(ptr, 0x60), 0) // padding for key
                mstore(add(ptr, 0x80), 0) // padding for key
                
                // Copy rest bytes from buffer
                let bufferPtr := add(buffer, 0x20)
                for { let j := 0 } lt(j, rest) { j := add(j, 0x20) } {
                    mstore(add(add(ptr, 0xa0), j), mload(add(bufferPtr, j)))
                }
                
                // Call set_storage syscall (0x1006)
                let result := call(gas(), 0x1006, 0, ptr, add(0xa0, rest), 0, 0)
            }
        }
        
        // Call the callee contract
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::empty() = 0
            mstore(ptr, 0)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee)
            
            // ref_time weight: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size weight: u64::MAX  
            mstore(add(ptr, 0x60), 0xffffffffffffffff)
            
            // No deposit limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x80), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // Value transferred (32 bytes of 0)
            mstore(add(ptr, 0xa0), 0)
            
            // Input data (4 bytes)
            mstore(add(ptr, 0xc0), mload(add(input, 0x20)))
            
            // Call the call syscall (0x3000)
            let result := call(gas(), 0x3000, 0, ptr, 0xe0, 0, 0)
            
            // Check if call succeeded
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
}