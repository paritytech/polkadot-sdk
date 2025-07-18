// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract DelegateCallDepositLimit {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: address (20 bytes), deposit_limit (u64)
        require(msg.data.length >= 28, "Invalid input length");
        
        address target = address(bytes20(msg.data[0:20]));
        uint64 deposit_limit = uint64(bytes8(msg.data[20:28]));
        
        // Convert u64 to u256_bytes (32 bytes)
        bytes32 deposit_limit_bytes = bytes32(uint256(deposit_limit));
        
        bytes memory input = new bytes(0);
        
        // Delegate call with deposit limit
        bool success = delegateCallWithDepositLimit(target, deposit_limit_bytes, input);
        
        if (!success) {
            // Return error code as revert data
            assembly {
                let ptr := mload(0x40)
                // Return error code (assuming OutOfStorage = 3)
                mstore(ptr, 3)
                revert(ptr, 4)
            }
        }
        
        // Check storage value
        bytes32 key = bytes32(uint256(1));
        bytes32 value = getStorage(key);
        
        require(uint8(value[0]) == 1, "Storage value should be 1");
    }
    
    function delegateCallWithDepositLimit(
        address target,
        bytes32 deposit_limit,
        bytes memory input
    ) internal returns (bool success) {
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
            
            // deposit_limit (32 bytes)
            mstore(add(ptr, 0x50), deposit_limit)
            
            // input data
            let input_len := mload(input)
            let input_ptr := add(input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x70), i), mload(add(input_ptr, i)))
            }
            
            // Call the delegate_call syscall (0x3002)
            let result := call(gas(), 0x3002, 0, ptr, add(0x70, input_len), 0, 0)
            success := result
        }
    }
    
    function getStorage(bytes32 key) internal returns (bytes32 value) {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0) // StorageFlags::empty()
            mstore(add(ptr, 0x20), key)
            
            // Call get_storage syscall (0x1005)
            let result := call(gas(), 0x1005, 0, ptr, 0x40, ptr, 0x20)
            value := mload(ptr)
        }
    }
}