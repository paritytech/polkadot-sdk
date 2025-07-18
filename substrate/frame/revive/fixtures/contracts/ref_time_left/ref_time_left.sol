// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract RefTimeLeft {
    constructor() {
        // In Solidity, we use gasleft() to get remaining gas
        // This assertion checks that gas is consumed during execution
        uint256 gas1 = gasleft();
        // Do some work to consume gas
        uint256 dummy = 0;
        for (uint i = 0; i < 10; i++) {
            dummy += i;
        }
        uint256 gas2 = gasleft();
        require(gas1 > gas2, "Gas should decrease");
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