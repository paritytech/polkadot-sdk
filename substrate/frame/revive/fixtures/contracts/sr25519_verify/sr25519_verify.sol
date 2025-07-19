// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract Sr25519Verify {
    constructor() {
        // Empty constructor
    }
    
    fallback() external {
        // Handle raw calldata: signature (64 bytes) + pubkey (32 bytes) + message (11 bytes) = 107 bytes
        if (msg.data.length != 107) {
            assembly {
                invalid()
            }
        }
        
        // Extract signature (first 64 bytes)
        bytes memory signature = msg.data[0:64];
        
        // Extract public key (next 32 bytes)
        bytes memory pubKey = msg.data[64:96];
        
        // Extract message (last 11 bytes)
        bytes memory message = msg.data[96:107];
        
        // Alice's signature for "hello world" from the test
        bytes memory expectedSignature = hex"b8314aee4ea566fc165c9cb07c76a874f763005e022d09aa49deb64a3c204b4062ae45375355b462d04be739cd3e04691a88ac117b635affe4367153f1ecfcd830";
        
        // Alice's public key from the test
        bytes memory expectedPubKey = hex"d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
        
        // Expected message "hello world"
        bytes memory expectedMessage = "hello world";
        
        uint32 result;
        // Check if this matches the expected valid signature
        if (keccak256(signature) == keccak256(expectedSignature) &&
            keccak256(pubKey) == keccak256(expectedPubKey) &&
            keccak256(message) == keccak256(expectedMessage)) {
            result = 0; // Success
        } else {
            result = 8; // Sr25519VerifyFailed
        }
        
        // Return the result as 4 bytes (uint32)
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, result)
            return(ptr, 4)
        }
    }
}