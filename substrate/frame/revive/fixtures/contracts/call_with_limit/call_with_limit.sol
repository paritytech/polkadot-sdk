// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CallWithLimit {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: callee_addr (20 bytes), ref_time (u64), proof_size (u64), forwarded_input (rest)
        require(msg.data.length >= 32, "Invalid input length");
        
        address callee_addr = address(bytes20(msg.data[0:20]));
        uint64 ref_time = uint64(bytes8(msg.data[20:28]));
        uint64 proof_size = uint64(bytes8(msg.data[28:36]));
        bytes memory forwarded_input = msg.data[36:];
        
        // Call the callee with the specified limits
        callWithLimits(callee_addr, ref_time, proof_size, forwarded_input);
    }
    
    function callWithLimits(
        address callee_addr,
        uint64 ref_time,
        uint64 proof_size,
        bytes memory forwarded_input
    ) internal {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::empty() = 0
            mstore(ptr, 0)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee_addr)
            
            // ref_time
            mstore(add(ptr, 0x40), ref_time)
            
            // proof_size
            mstore(add(ptr, 0x48), proof_size)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x50), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes of 0)
            mstore(add(ptr, 0x70), 0)
            
            // forwarded_input data
            let input_len := mload(forwarded_input)
            let input_ptr := add(forwarded_input, 0x20)
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