// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract OomRwIncluded {
    // This creates a large rw section but with its contents
    // included into the blob. It should be rejected for its
    // blob size.
    
    bytes buffer;
    
    constructor() {
        // Initialize buffer with large size (smaller for testing) and value 42
        buffer = new bytes(1024);
        for (uint256 i = 0; i < 1024; i++) {
            buffer[i] = 0x2a; // 42 in hex
        }
    }
    
    function callNever() external {
        // make sure the buffer is not optimized away
        bytes memory buf = buffer;
        assembly {
            let ptr := mload(0x40)
            let buffer_ptr := add(buf, 0x20)
            let buffer_len := mload(buf)
            for { let i := 0 } lt(i, buffer_len) { i := add(i, 0x20) } {
                mstore(add(ptr, i), mload(add(buffer_ptr, i)))
            }
            return(ptr, buffer_len)
        }
    }
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Empty fallback function
    }
}