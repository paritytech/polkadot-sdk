// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CallReturnCode {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: callee_addr (20 bytes), value (32 bytes), input data (rest)
        require(msg.data.length >= 52, "Invalid input length");
        
        address callee_addr = address(bytes20(msg.data[0:20]));
        bytes32 value = bytes32(msg.data[20:52]);
        bytes memory input = msg.data[52:];
        
        // Call the callee
        uint32 err_code = callAndGetErrorCode(callee_addr, value, input);
        
        // Return the error code
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, err_code)
            return(ptr, 4)
        }
    }
    
    function callAndGetErrorCode(
        address callee_addr,
        bytes32 value,
        bytes memory input
    ) internal returns (uint32 err_code) {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::empty() = 0
            mstore(ptr, 0)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee_addr)
            
            // ref_time: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x48), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x50), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes)
            mstore(add(ptr, 0x70), value)
            
            // input data
            let input_len := mload(input)
            let input_ptr := add(input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x90), i), mload(add(input_ptr, i)))
            }
            
            // Call the call syscall (0x3000)
            let result := call(gas(), 0x3000, 0, ptr, add(0x90, input_len), 0, 0)
            
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