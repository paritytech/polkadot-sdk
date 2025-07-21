// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract CallReturnCode {
    constructor() payable {}

    fallback() external payable {
        // Input format:
        // [0..20)   → callee (20 bytes)
        // [20..52)  → value  (32 bytes)
        // [52..]    → calldata for inner call

        // Extract callee address (20 bytes)
        address callee = address(bytes20(msg.data[0:20]));

        // Extract value (32 bytes)
        uint256 value = uint256(bytes32(msg.data[20:52]));

        // Extract inner call data
        bytes memory callData = msg.data[52:];

        // Make the call and capture return data
        (bool success, bytes memory returnData) = callee.call{value: value}(callData);

        // Determine return code based on call result
        bytes4 returnCode;

		if (!success) {
            if (returnData.length > 0) {
                returnCode = hex"02000000"; // Callee reverted
            } else if (address(this).balance < value) {
                returnCode = hex"04000000"; // Transfer failed
            } else {
                returnCode = hex"01000000"; // Callee trapped
            }
        } else {
            returnCode = hex"00000000"; // Success
        }

        assembly {
            return(add(returnCode, 0x20), 4)
        }
    }

}


