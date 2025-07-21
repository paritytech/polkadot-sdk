// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract Recurse {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: calls_left (u32)
        require(msg.data.length >= 4, "Invalid input length");
        
        uint32 calls_left = uint32(bytes4(msg.data[0:4]));
        
        // Get own address
        address addr = address(this);
        
        if (calls_left == 0) {
            return;
        }
        
        // Call self recursively with calls_left - 1
        callSelfRecursively(addr, calls_left - 1);
    }
    
    function callSelfRecursively(address addr, uint32 calls_left) internal {
        assembly {
            let ptr := mload(0x40)
            
            // CallFlags::ALLOW_REENTRY = 1
            mstore(ptr, 1)
            
            // Target address (20 bytes)
            mstore(add(ptr, 0x20), addr)
            
            // ref_time: u64::MAX
            mstore(add(ptr, 0x40), 0xffffffffffffffff)
            
            // proof_size: u64::MAX
            mstore(add(ptr, 0x48), 0xffffffffffffffff)
            
            // deposit_limit (32 bytes of 0xFF)
            mstore(add(ptr, 0x50), 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            
            // value (32 bytes of 0)
            mstore(add(ptr, 0x70), 0)
            
            // input data - calls_left as little-endian u32
            mstore(add(ptr, 0x90), calls_left)
            
            // Call the call syscall (0x3000)
            let result := call(gas(), 0x3000, 0, ptr, 0x94, 0, 0)
            
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
}