// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract OomRwTrailing {
    // This creates a large rw section but the trailing zeroes
    // are removed by the linker. It should be rejected even
    // though the blob is small enough.
    
    bytes buffer;
    
    constructor() {
        // Initialize buffer with large size (smaller for testing)
        buffer = new bytes(1024);
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