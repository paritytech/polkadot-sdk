// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract RefTimeLeft {
    constructor() {
        // In Solidity, we use gasleft() to get remaining gas
        // This assertion checks that gas is consumed during execution
        uint256 gas1 = gasleft();
        uint256 gas2 = gasleft();
        // Check that gas decreases between calls (like the Rust version)
        assembly {
            if iszero(gt(gas1, gas2)) {
                invalid()
            }
        }
    }
    
    fallback() external payable {
        // Return the remaining gas as a 64-bit value in little-endian format
        uint64 refTimeLeft = uint64(gasleft());
        assembly {
            mstore(0x00, refTimeLeft)
            return(0x00, 0x08)
        }
    }
}