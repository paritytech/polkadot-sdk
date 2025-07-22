// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract DelegateCall {
    constructor() payable {
        // Call during deployment as well, like deploy() calling call() in Rust
        _processDelegateCall();
    }
    
    function _processDelegateCall() internal {
        // Input format from Rust version:
        // [0..20)   → address (20 bytes)
        // [20..28)  → ref_time (8 bytes) - ignored in Solidity
        // [28..36)  → proof_size (8 bytes) - ignored in Solidity
        
        bytes calldata inputData = msg.data;
        
        if (inputData.length < 20) {
            return; // Need at least 20 bytes for address
        }
        
        // Extract delegate call target address (first 20 bytes)
        address target = address(bytes20(inputData[0:20]));
        
        // Set up storage operations like in Rust version
        bytes32 key = bytes32(uint256(1));  // key[0] = 1u8
        bytes32 initialValue = bytes32(uint256(2)); // value[0] = 2u8
        
        // Set initial storage: api::set_storage()
        assembly {
            sstore(key, initialValue)
        }
        
        // Verify initial storage: api::get_storage() + assert
        assembly {
            let storedValue := sload(key)
            if iszero(eq(storedValue, initialValue)) {
                revert(0, 0)
            }
        }
        
        // Perform delegate call with empty input: api::delegate_call()
        (bool success, ) = target.delegatecall("");
        
        if (!success) {
            revert();
        }
        
        // Verify storage was modified to 1: api::get_storage() + assert  
        assembly {
            let finalValue := sload(key)
            if iszero(eq(finalValue, 1)) {
                revert(0, 0)
            }
        }
    }
    
    fallback() external payable {
        _processDelegateCall();
    }
}