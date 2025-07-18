// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract DestroyAndTransfer {
    bytes32 constant ADDRESS_KEY = bytes32(0);
    uint256 constant VALUE = 65536;
    
    function deploy() external {
        // Parse input: code_hash (32 bytes)
        require(msg.data.length >= 36, "Invalid input length");
        
        bytes32 code_hash = bytes32(msg.data[4:36]);
        
        bytes32 salt = bytes32(uint256(0x2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f));
        
        // Instantiate the contract
        address deployed_address = instantiateContract(code_hash, salt);
        
        // Store the deployed contract address
        setStorage(ADDRESS_KEY, bytes32(uint256(uint160(deployed_address))));
    }
    
    fallback() external payable {
        // Get the stored callee address
        bytes32 stored_address = getStorage(ADDRESS_KEY);
        address callee_addr = address(uint160(uint256(stored_address)));
        
        // Calling the destination contract with non-empty input data should fail.
        bool success = callContract(callee_addr, VALUE, bytes("0"));
        require(!success, "Expected call to fail");
        
        // Call the destination contract regularly, forcing it to self-destruct.
        success = callContract(callee_addr, VALUE, bytes(""));
        require(success, "Expected call to succeed");
    }
    
    function instantiateContract(bytes32 code_hash, bytes32 salt) internal returns (address deployed_address) {
        assembly {
            let ptr := mload(0x40)
            
            // ref_time_limit: u64::MAX
            mstore(ptr, 0xffffffffffffffff)
            
            // proof_size_limit: u64::MAX
            mstore(add(ptr, 0x08), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x10), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes)
            mstore(add(ptr, 0x30), VALUE)
            
            // code_hash (32 bytes)
            mstore(add(ptr, 0x50), code_hash)
            
            // salt (32 bytes)
            mstore(add(ptr, 0x70), salt)
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, 0x90, ptr, 0x20)
            
            if iszero(result) {
                revert(0, 0)
            }
            
            deployed_address := mload(ptr)
        }
    }
    
    function callContract(address target, uint256 value, bytes memory input) internal returns (bool success) {
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
            success := result
        }
    }
    
    function setStorage(bytes32 key, bytes32 value) internal {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0) // StorageFlags::empty()
            mstore(add(ptr, 0x20), key)
            mstore(add(ptr, 0x40), value)
            
            // Call set_storage syscall (0x1006)
            let result := call(gas(), 0x1006, 0, ptr, 0x60, 0, 0)
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