// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract DelegateCallSimple {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: address (20 bytes)
        require(msg.data.length >= 20, "Invalid input length");
        
        address target = address(bytes20(msg.data[0:20]));
        
        // Delegate call into passed address.
        bytes memory input = new bytes(0);
        bytes memory output = delegateCallWithOutput(target, input);
        
        // Assert output length is 0
        require(output.length == 0, "Output length should be 0");
    }
    
    function delegateCallWithOutput(
        address target,
        bytes memory input
    ) internal returns (bytes memory output) {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::empty() = 0
            mstore(ptr, 0)
            
            // Target address (20 bytes)
            mstore(add(ptr, 0x20), target)
            
            // ref_time_limit: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size_limit: u64::MAX
            mstore(add(ptr, 0x48), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x50), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // input data
            let input_len := mload(input)
            let input_ptr := add(input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x70), i), mload(add(input_ptr, i)))
            }
            
            // Call the delegate_call syscall (0x3002)
            let result := call(gas(), 0x3002, 0, ptr, add(0x70, input_len), ptr, 0x200)
            
            if iszero(result) {
                revert(0, 0)
            }
            
            // Get output length and data
            let output_len := mload(ptr)
            output := mload(0x40)
            mstore(output, output_len)
            
            // Copy output data
            for { let i := 0 } lt(i, output_len) { i := add(i, 0x20) } {
                mstore(add(add(output, 0x20), i), mload(add(add(ptr, 0x20), i)))
            }
            
            mstore(0x40, add(add(output, 0x20), output_len))
        }
    }
}