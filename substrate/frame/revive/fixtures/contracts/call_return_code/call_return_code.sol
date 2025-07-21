// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CallReturnCode {
    constructor() payable {}

    fallback() external payable {
        assembly {
            // Function to convert a 32-byte little-endian value to big-endian
            function leToBe(input) -> output {
                output := 0
                for { let i := 0 } lt(i, 32) { i := add(i, 1) } {
                    let byteVal := and(shr(mul(i, 8), input), 0xff)
                    output := or(output, shl(mul(sub(31, i), 8), byteVal))
                }
            }

            // Input format:
            // [0..20)   → callee (20 bytes)
            // [20..52)  → value  (32 bytes, little-endian)
            // [52..]    → calldata for inner call

            // Copy 20-byte callee address
            calldatacopy(0x00, 0x00, 20)
            let callee := shr(96, mload(0x00))

            // Copy and convert 32-byte value
            calldatacopy(0x00, 20, 32)
            let value := leToBe(mload(0x00))

            // Set input offset and size
            let inputOffset := 52
            let inputSize := sub(calldatasize(), inputOffset)

            // Call target with input and value
            let success := call(gas(), callee, value, inputOffset, inputSize, 0x00, 0x20)

            switch success
            case 1 {
                mstore(0x00, 0)
                return(0x00, 0x04)
            }
            default {
                returndatacopy(0x00, 0x00, 0x04)
                return(0x00, 0x04)
            }
        }
    }
}
