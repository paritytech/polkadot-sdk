// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CallWithFlagsAndValue {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: callee_addr (20 bytes), flags (u32), value (u64), forwarded_input (rest)
        require(msg.data.length >= 32, "Invalid input length");
        
        address callee_addr = address(bytes20(msg.data[0:20]));
        uint32 flags = uint32(bytes4(msg.data[20:24]));
        uint64 value = uint64(bytes8(msg.data[24:32]));
        bytes memory forwarded_input = msg.data[32:];
        
        // Call the callee with the specified flags and value
        callWithFlagsAndValue(callee_addr, flags, value, forwarded_input);
    }
    
    function callWithFlagsAndValue(
        address callee_addr,
        uint32 flags,
        uint64 value,
        bytes memory forwarded_input
    ) internal {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags
            mstore(ptr, flags)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee_addr)
            
            // ref_time: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x48), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x50), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (converted to u256)
            mstore(add(ptr, 0x70), value)
            
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