// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract Create1WithValue {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: code_hash (32 bytes)
        require(msg.data.length >= 32, "Invalid input length");
        
        bytes32 code_hash = bytes32(msg.data[0:32]);
        
        // Get value transferred
        uint256 value = getValueTransferred();
        
        // Deploy the contract with no salt (equivalent to create1)
        instantiateWithValue(code_hash, value);
    }
    
    function getValueTransferred() internal returns (uint256 value) {
        assembly {
            let ptr := mload(0x40)
            
            // Call value_transferred syscall (0x1003)
            let result := call(gas(), 0x1003, 0, 0, 0, ptr, 0x20)
            
            if iszero(result) {
                revert(0, 0)
            }
            
            value := mload(ptr)
        }
    }
    
    function instantiateWithValue(bytes32 code_hash, uint256 value) internal {
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
            
            // No salt (None), so we don't include it in the call
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, 0x70, 0, 0)
            
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
}