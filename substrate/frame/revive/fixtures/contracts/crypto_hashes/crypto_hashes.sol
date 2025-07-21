// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CryptoHashes {
    constructor() {
        // Empty constructor
    }
    
    function call(uint8 chosen_hash_fn, bytes calldata input) external returns (bytes memory) {
        if (chosen_hash_fn == 2) {
            // KECCAK256
            bytes32 result = keccak256(input);
            return abi.encodePacked(result);
        } else {
            assembly {
                invalid()
            }
        }
    }
}