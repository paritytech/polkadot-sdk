// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Create2WithValue {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: code_hash (32 bytes)
        if (msg.data.length < 32) {
            assembly { invalid() }
        }
        
        bytes32 code_hash = bytes32(msg.data[0:32]);
        
        // Get value transferred
        uint256 value = msg.value;
        
        // Deploy the contract with salt (equivalent to create2)
        bytes32 salt = bytes32(uint256(1));
        
        assembly {
            let ptr := mload(0x40)
            
            // ref_time: u64::MAX
            mstore(ptr, 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x08), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x10), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes)
            mstore(add(ptr, 0x30), value)
            
            // code_hash (32 bytes)
            mstore(add(ptr, 0x50), code_hash)
            
            // salt (32 bytes)
            mstore(add(ptr, 0x70), salt)
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, 0x90, 0, 0)
            
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
}