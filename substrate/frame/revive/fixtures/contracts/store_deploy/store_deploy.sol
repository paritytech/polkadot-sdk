// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract StoreDeploy {
    // Buffer size is 16 * 1024 + 1
    uint256 constant BUFFER_SIZE = 16385;
    
    function deploy() external {
        // Parse input: len (u32)
        require(msg.data.length >= 8, "Invalid input length");
        
        uint32 len = uint32(bytes4(msg.data[4:8]));
        
        // Create storage data filled with zeros
        bytes32 key = bytes32(uint256(1));
        bytes memory data = new bytes(len);
        // Data is already filled with zeros by default
        
        // Place a garbage value in storage, the size of which is specified by the call input
        setStorage(key, data);
    }
    
    fallback() external payable {
        // Empty fallback function
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
}