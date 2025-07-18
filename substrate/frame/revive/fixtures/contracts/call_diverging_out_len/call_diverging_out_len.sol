// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CallDivergingOutLen {
    bytes8 constant DATA = hex"0102030405060708";
    
    function deploy() external {
        // Return DATA on deploy
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0x0102030405060708000000000000000000000000000000000000000000000000)
            return(ptr, 8)
        }
    }
    
    fallback() external payable {
        address caller = getCaller();
        address callee = address(this);
        
        // If we already recurse, return data
        if (caller == callee) {
            assembly {
                let ptr := mload(0x40)
                mstore(ptr, 0x0102030405060708000000000000000000000000000000000000000000000000)
                return(ptr, 8)
            }
        }
        
        // Test calls with different output buffer sizes
        assertCall(callee, 0);
        assertCall(callee, 4);
        
        // Test instantiate with different output buffer sizes
        assertInstantiate(0);
        assertInstantiate(4);
    }
    
    function getCaller() internal returns (address caller_addr) {
        assembly {
            let ptr := mload(0x40)
            // Call caller syscall (0x1001)
            let result := call(gas(), 0x1001, 0, 0, 0, ptr, 0x20)
            caller_addr := mload(ptr)
        }
    }
    
    function assertCall(address callee_address, uint256 output_size) internal {
        // Make a call with specified output buffer size
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::ALLOW_REENTRY = 1
            mstore(ptr, 1)
            
            // Callee address
            mstore(add(ptr, 0x20), callee_address)
            
            // ref_time: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x48), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x50), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes of 0)
            mstore(add(ptr, 0x70), 0)
            
            // Empty input
            // No input data
            
            // Call the call syscall (0x3000)
            let result := call(gas(), 0x3000, 0, ptr, 0x90, ptr, output_size)
            
            // Basic assertion - if we got here, the call succeeded
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
    
    function assertInstantiate(uint256 output_size) internal {
        bytes32 code_hash = getOwnCodeHash();
        
        // Make an instantiate call with specified output buffer size
        assembly {
            let ptr := mload(0x40)
            
            // ref_time: u64::MAX
            mstore(ptr, 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x08), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x10), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes of 0)
            mstore(add(ptr, 0x30), 0)
            
            // code_hash (32 bytes)
            mstore(add(ptr, 0x50), code_hash)
            
            // No salt (None)
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, 0x70, ptr, output_size)
            
            // Basic assertion - if we got here, the instantiate succeeded
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
    
    function getOwnCodeHash() internal returns (bytes32 code_hash) {
        assembly {
            let ptr := mload(0x40)
            
            // Call own_code_hash syscall (0x1013)
            let result := call(gas(), 0x1013, 0, 0, 0, ptr, 0x20)
            code_hash := mload(ptr)
        }
    }
}