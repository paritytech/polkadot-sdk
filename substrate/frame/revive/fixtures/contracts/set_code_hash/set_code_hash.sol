// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract SetCodeHash {
    
    function deploy() external {
        // Empty deploy function
    }
    
    fallback() external payable {
        // Parse input: addr (32 bytes)
        require(msg.data.length >= 32, "Invalid input length");
        
        bytes32 addr = bytes32(msg.data[0:32]);
        
        // Set code hash
        setCodeHash(addr);
        
        // Return 1 after setting new code_hash
        // Next `call` will NOT return this value, because contract code has been changed
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 1)
            return(ptr, 4)
        }
    }
    
    function setCodeHash(bytes32 addr) internal {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, addr)
            
            // Call set_code_hash syscall (0x2002)
            let result := call(gas(), 0x2002, 0, ptr, 0x20, 0, 0)
            
            if iszero(result) {
                revert(0, 0)
            }
        }
    }
}