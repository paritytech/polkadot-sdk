// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CreateStorageAndInstantiate {
    // Buffer size is 16 * 1024 + 1
    uint256 constant BUFFER_SIZE = 16385;
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: code_hash (32 bytes), input (4 bytes), deposit_limit (32 bytes)
        if (msg.data.length < 68) {
            assembly { invalid() }
        }
        
        bytes32 code_hash = bytes32(msg.data[0:32]);
        bytes4 input = bytes4(msg.data[32:36]);
        bytes32 deposit_limit = bytes32(msg.data[36:68]);
        
        // Parse the length from input
        uint32 len = uint32(input);
        
        // Create storage data
        bytes32 key = bytes32(uint256(1));
        bytes memory data = new bytes(len);
        // Fill with zeros (as per BUFFER)
        setStorage(key, data);
        
        // Prepare deploy input
        bytes memory deploy_input = new bytes(36);
        for (uint i = 0; i < 32; i++) {
            deploy_input[i] = code_hash[i];
        }
        for (uint i = 0; i < 4; i++) {
            deploy_input[32 + i] = input[i];
        }
        
        // Instantiate the contract
        address deployed_address = instantiateContract(deposit_limit, deploy_input);
        
        if (deployed_address == address(0)) {
            assembly {
                let ptr := mload(0x40)
                // Return error code (assuming generic error = 1)
                mstore(ptr, 1)
                revert(ptr, 4)
            }
        }
        
        // Return the deployed contract address
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, deployed_address)
            return(ptr, 20)
        }
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
    
    function instantiateContract(
        bytes32 deposit_limit,
        bytes memory deploy_input
    ) internal returns (address deployed_address) {
        assembly {
            let ptr := mload(0x40)
            
            // ref_time: u64::MAX
            mstore(ptr, 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x08), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes)
            mstore(add(ptr, 0x10), deposit_limit)
            
            // value (10_000 as u256)
            mstore(add(ptr, 0x30), 10000)
            
            // deploy_input data
            let input_len := mload(deploy_input)
            let input_ptr := add(deploy_input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x50), i), mload(add(input_ptr, i)))
            }
            
            // salt (32 bytes of 0)
            mstore(add(add(ptr, 0x50), input_len), 0)
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, add(0x70, input_len), ptr, 0x20)
            
            switch result
            case 0 {
                deployed_address := 0
            }
            default {
                deployed_address := mload(ptr)
            }
        }
    }
}