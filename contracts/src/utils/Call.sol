// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2023 OpenZeppelin
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.23;

// Derived from OpenZeppelin Contracts (last updated v4.9.0) (utils/Address.sol)
library Call {
    function verifyResult(bool success, bytes memory returndata) internal pure returns (bytes memory) {
        if (success) {
            return returndata;
        } else {
            // Look for revert reason and bubble it up if present
            if (returndata.length > 0) {
                // The easiest way to bubble the revert reason is using memory via assembly
                /// @solidity memory-safe-assembly
                assembly {
                    let returndata_size := mload(returndata)
                    revert(add(32, returndata), returndata_size)
                }
            } else {
                revert();
            }
        }
    }
}
