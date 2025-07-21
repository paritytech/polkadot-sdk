// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract ReadOnlyCall {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: callee_addr (20 bytes), callee_input (rest)
        require(msg.data.length >= 20, "Invalid input length");
        
        address callee_addr = address(bytes20(msg.data[0:20]));
        bytes memory callee_input = msg.data[20:];
        
        // Call the callee with READ_ONLY flag
        callReadOnly(callee_addr, callee_input);
    }
    
    function callReadOnly(address callee_addr, bytes memory callee_input) internal {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::READ_ONLY = 2
            mstore(ptr, 2)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee_addr)
            
            // ref_time: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x48), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x50), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes of 0)
            mstore(add(ptr, 0x70), 0)
            
            // callee_input data
            let input_len := mload(callee_input)
            let input_ptr := add(callee_input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x90), i), mload(add(input_ptr, i)))
            }
            
            // Call the call syscall (0x3000)
            let result := call(gas(), 0x3000, 0, ptr, add(0x90, input_len), 0, 0)
            
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
}