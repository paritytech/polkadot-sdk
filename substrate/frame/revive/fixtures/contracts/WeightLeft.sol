// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.20;

import "@revive/ISystem.sol";

contract WeightLeft {
    function weightLeft() external view returns (uint64 refTime, uint64 proofSize) {
        return ISystem(SYSTEM_ADDR).weightLeft();
    }
}
