// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract OomRo {
    // This creates a large ro section. Even though it is zero
    // initialized we expect them to be included into the blob.
    // This means it will fail at the blob size check.
    
    bytes buffer;
    
    constructor() {
        // Initialize buffer with large size (smaller for testing) of zeros
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