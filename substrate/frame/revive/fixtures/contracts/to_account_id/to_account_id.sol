// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ToAccountId {
    constructor() {
        // Empty constructor
    }
    
    fallback() external payable {
        // Parse input: address (20 bytes), expected_account_id (32 bytes)
        require(msg.data.length >= 52, "Invalid input length");
        
        address addr = address(bytes20(msg.data[0:20]));
        bytes32 expected_account_id = bytes32(msg.data[20:52]);
        
        // Get account ID using syscall
        bytes32 account_id;
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, addr)
            
            // Call to_account_id syscall (0x1014)
            let result := call(gas(), 0x1014, 0, ptr, 0x20, ptr, 0x20)
            
            if iszero(result) {
                revert(0, 0)
            }
            
            account_id := mload(ptr)
        }
        
        require(account_id == expected_account_id, "Account ID mismatch");
    }
}