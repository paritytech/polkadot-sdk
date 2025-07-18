// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CreateStorageAndCall {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: buffer (32 bytes), input (4 bytes), callee (20 bytes), deposit_limit (32 bytes)
        require(msg.data.length >= 88, "Invalid input length");
        
        bytes32 buffer = bytes32(msg.data[0:32]);
        bytes4 input = bytes4(msg.data[32:36]);
        address callee = address(bytes20(msg.data[36:56]));
        bytes32 deposit_limit = bytes32(msg.data[56:88]);
        
        // Create 4 bytes of storage before calling
        bytes memory value_4 = new bytes(4);
        for (uint i = 0; i < 4; i++) {
            value_4[i] = bytes1(uint8(1));
        }
        setStorage(buffer, value_4);
        
        // Call the callee
        bool success = callContractWithDepositLimit(callee, deposit_limit, abi.encodePacked(input));
        
        if (!success) {
            assembly {
                let ptr := mload(0x40)
                // Return error code (assuming generic error = 1)
                mstore(ptr, 1)
                revert(ptr, 4)
            }
        }
        
        // Create 8 bytes of storage after calling
        // Item of 12 bytes because we override 4 bytes
        bytes memory value_12 = new bytes(12);
        for (uint i = 0; i < 12; i++) {
            value_12[i] = bytes1(uint8(1));
        }
        setStorage(buffer, value_12);
    }
    
    function setStorage(bytes32 key, bytes memory value) internal {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0) // StorageFlags::empty()
            mstore(add(ptr, 0x20), key)
            
            // Copy value data
            let value_len := mload(value)
            let value_ptr := add(value, 0x20)
            for { let i := 0 } lt(i, value_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x40), i), mload(add(value_ptr, i)))
            }
            
            // Call set_storage syscall (0x1006)
            let result := call(gas(), 0x1006, 0, ptr, add(0x40, value_len), 0, 0)
        }
    }
    
    function callContractWithDepositLimit(
        address callee,
        bytes32 deposit_limit,
        bytes memory input
    ) internal returns (bool success) {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::empty() = 0
            mstore(ptr, 0)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee)
            
            // ref_time: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x48), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes)
            mstore(add(ptr, 0x50), deposit_limit)
            
            // value (32 bytes of 0)
            mstore(add(ptr, 0x70), 0)
            
            // input data
            let input_len := mload(input)
            let input_ptr := add(input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x90), i), mload(add(input_ptr, i)))
            }
            
            // Call the call syscall (0x3000)
            let result := call(gas(), 0x3000, 0, ptr, add(0x90, input_len), 0, 0)
            success := result
        }
    }
}