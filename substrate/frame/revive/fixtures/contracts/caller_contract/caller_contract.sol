// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract CallerContract {
    bytes constant INPUT = hex"0001223344556677";
    bytes constant REVERTED_INPUT = hex"01223344556677";
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: code_hash (32 bytes), load_code_ref_time (u64), load_code_proof_size (u64)
        require(msg.data.length >= 48, "Invalid input length");
        
        bytes32 code_hash = bytes32(msg.data[0:32]);
        uint64 load_code_ref_time = uint64(bytes8(msg.data[32:40]));
        uint64 load_code_proof_size = uint64(bytes8(msg.data[40:48]));
        
        // The value to transfer on instantiation and calls. Chosen to be greater than existential deposit.
        uint256 value = 32768;
        bytes32 salt = bytes32(0);
        
        // Callee will use the first 4 bytes of the input to return an exit status.
        bytes memory input_deploy = new bytes(32 + INPUT.length);
        for (uint256 i = 0; i < 32; i++) {
            input_deploy[i] = code_hash[i];
        }
        for (uint256 i = 0; i < INPUT.length; i++) {
            input_deploy[32 + i] = INPUT[i];
        }
        
        bytes memory reverted_input_deploy = new bytes(32 + REVERTED_INPUT.length);
        for (uint256 i = 0; i < 32; i++) {
            reverted_input_deploy[i] = code_hash[i];
        }
        for (uint256 i = 0; i < REVERTED_INPUT.length; i++) {
            reverted_input_deploy[32 + i] = REVERTED_INPUT[i];
        }
        
        // Fail to deploy the contract since it returns a non-zero exit status.
        bool success = instantiate(
            0xffffffffffffffff, // u64::MAX ref_time weight
            0xffffffffffffffff, // u64::MAX proof_size weight  
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            reverted_input_deploy,
            salt
        );
        require(!success, "Expected instantiate to fail");
        
        // Fail to deploy the contract due to insufficient ref_time weight.
        success = instantiate(
            1, // too little ref_time weight
            0xffffffffffffffff, // u64::MAX proof_size weight
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            input_deploy,
            salt
        );
        require(!success, "Expected instantiate to fail due to insufficient ref_time weight");
        
        // Fail to deploy the contract due to insufficient proof_size weight.
        success = instantiate(
            0xffffffffffffffff, // u64::MAX ref_time weight
            1, // Too little proof_size weight
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            input_deploy,
            salt
        );
        require(!success, "Expected instantiate to fail due to insufficient proof_size weight");
        
        // Deploy the contract successfully.
        address callee = instantiateAndGetAddress(
            0xffffffffffffffff, // u64::MAX ref_time weight
            0xffffffffffffffff, // u64::MAX proof_size weight
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            input_deploy,
            salt
        );
        
        // Call the new contract and expect it to return failing exit code.
        success = callContract(
            callee,
            0xffffffffffffffff, // u64::MAX ref_time weight
            0xffffffffffffffff, // u64::MAX proof_size weight
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            REVERTED_INPUT
        );
        require(!success, "Expected call to fail");
        
        // Fail to call the contract due to insufficient ref_time weight.
        success = callContract(
            callee,
            load_code_ref_time,   // just enough to load the contract
            load_code_proof_size, // just enough to load the contract
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            INPUT
        );
        require(!success, "Expected call to fail due to insufficient ref_time weight");
        
        // Fail to call the contract due to insufficient proof_size weight.
        success = callContract(
            callee,
            0xffffffffffffffff, // u64::MAX ref_time weight
            load_code_proof_size, // just enough to load the contract
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            INPUT
        );
        require(!success, "Expected call to fail due to insufficient proof_size weight");
        
        // Call the contract successfully.
        bytes memory output = callContractAndGetOutput(
            callee,
            0xffffffffffffffff, // u64::MAX ref_time weight
            0xffffffffffffffff, // u64::MAX proof_size weight
            bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), // No deposit limit
            value,
            INPUT
        );
        
        // Verify output matches expected (INPUT[4..])
        require(output.length == 4, "Output length should be 4");
        for (uint256 i = 0; i < 4; i++) {
            require(output[i] == INPUT[i + 4], "Output mismatch");
        }
    }
    
    function instantiate(
        uint64 ref_time_limit,
        uint64 proof_size_limit,
        bytes32 deposit_limit,
        uint256 value,
        bytes memory input,
        bytes32 salt
    ) internal returns (bool success) {
        assembly {
            let ptr := mload(0x40)
            
            // ref_time_limit (u64)
            mstore(ptr, ref_time_limit)
            
            // proof_size_limit (u64)
            mstore(add(ptr, 0x08), proof_size_limit)
            
            // deposit_limit (32 bytes)
            mstore(add(ptr, 0x10), deposit_limit)
            
            // value (32 bytes)
            mstore(add(ptr, 0x30), value)
            
            // input data
            let input_len := mload(input)
            let input_ptr := add(input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x50), i), mload(add(input_ptr, i)))
            }
            
            // salt (32 bytes)
            mstore(add(add(ptr, 0x50), input_len), salt)
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, add(0x70, input_len), 0, 0)
            success := result
        }
    }
    
    function instantiateAndGetAddress(
        uint64 ref_time_limit,
        uint64 proof_size_limit,
        bytes32 deposit_limit,
        uint256 value,
        bytes memory input,
        bytes32 salt
    ) internal returns (address callee) {
        assembly {
            let ptr := mload(0x40)
            
            // ref_time_limit (u64)
            mstore(ptr, ref_time_limit)
            
            // proof_size_limit (u64)
            mstore(add(ptr, 0x08), proof_size_limit)
            
            // deposit_limit (32 bytes)
            mstore(add(ptr, 0x10), deposit_limit)
            
            // value (32 bytes)
            mstore(add(ptr, 0x30), value)
            
            // input data
            let input_len := mload(input)
            let input_ptr := add(input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x50), i), mload(add(input_ptr, i)))
            }
            
            // salt (32 bytes)
            mstore(add(add(ptr, 0x50), input_len), salt)
            
            // Call instantiate syscall (0x3001)
            let result := call(gas(), 0x3001, 0, ptr, add(0x70, input_len), ptr, 0x20)
            
            if result {
                callee := mload(ptr)
            }
        }
    }
    
    function callContract(
        address callee,
        uint64 ref_time_limit,
        uint64 proof_size_limit,
        bytes32 deposit_limit,
        uint256 value,
        bytes memory input
    ) internal returns (bool success) {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::empty() = 0
            mstore(ptr, 0)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee)
            
            // ref_time_limit (u64)
            mstore(add(ptr, 0x40), ref_time_limit)
            
            // proof_size_limit (u64)
            mstore(add(ptr, 0x48), proof_size_limit)
            
            // deposit_limit (32 bytes)
            mstore(add(ptr, 0x50), deposit_limit)
            
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
    
    function callContractAndGetOutput(
        address callee,
        uint64 ref_time_limit,
        uint64 proof_size_limit,
        bytes32 deposit_limit,
        uint256 value,
        bytes memory input
    ) internal returns (bytes memory output) {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::empty() = 0
            mstore(ptr, 0)
            
            // Callee address (20 bytes)
            mstore(add(ptr, 0x20), callee)
            
            // ref_time_limit (u64)
            mstore(add(ptr, 0x40), ref_time_limit)
            
            // proof_size_limit (u64)
            mstore(add(ptr, 0x48), proof_size_limit)
            
            // deposit_limit (32 bytes)
            mstore(add(ptr, 0x50), deposit_limit)
            
            // value (32 bytes)
            mstore(add(ptr, 0x70), value)
            
            // input data
            let input_len := mload(input)
            let input_ptr := add(input, 0x20)
            for { let i := 0 } lt(i, input_len) { i := add(i, 0x20) } {
                mstore(add(add(ptr, 0x90), i), mload(add(input_ptr, i)))
            }
            
            // Call the call syscall (0x3000)
            let result := call(gas(), 0x3000, 0, ptr, add(0x90, input_len), ptr, 0x20)
            
            if result {
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
}