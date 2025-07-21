// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.0;

contract OkTrapRevert {

    constructor() payable {}

    fallback() external payable {
        // Check if there's input data
        if (msg.data.length >= 4) {
            uint8 first_byte = uint8(msg.data[0]);

            if (first_byte == 1) {
				revert("revert!");
            } else if (first_byte == 2) {
                // Panic/trap
                assembly {
                    invalid()
                }
            }
            // For other values, do nothing and return normally
        }
    }
}
