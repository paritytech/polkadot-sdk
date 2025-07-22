// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract ReturnWithData {
    constructor() payable {
        _processData();
    }

    function _processData() internal {
        // Input format from Rust version:
        // [0..4)   → exit_status (4 bytes)
        // [4..]    → output data

        bytes calldata inputData = msg.data;

        if (inputData.length < 4) {
            return; // Need at least 4 bytes for exit status
        }

        // Simulate storage operation for PoV consumption like in Rust version
        // This mimics api::clear_storage(StorageFlags::empty(), b"");
        assembly {
            let dummy := sload(0)
            sstore(0, 0)
        }

        // Extract output data (everything after first 4 bytes)
        bytes memory output;
        if (inputData.length > 4) {
            output = inputData[4:];
        }

        // Return the output data
        assembly {
            return(add(output, 0x20), mload(output))
        }
    }

    fallback() external payable {
        _processData();
    }
}
