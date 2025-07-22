// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ExtCodeSize {
    constructor() payable {}

    fallback() external payable {
        // Input format:
        // [0..20)   → address (20 bytes)
        // [20..52)  → expected (32 bytes as u256 )

        // Extract target address (20 bytes)
        address target = address(bytes20(msg.data[0:20]));

        // Extract expected size (8 bytes as u64, little endian)
        uint256 expected = uint256(bytes32(msg.data[20:52]));

        uint256 codeSize;
        assembly {
            codeSize := extcodesize(target)
        }

        if (codeSize != expected) {
            assembly { invalid() }
        }
    }
}
