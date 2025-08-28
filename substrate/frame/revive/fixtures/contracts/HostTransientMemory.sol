// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract HostTransientMemory {
    function transientMemoryTest(
        uint256 slot,
        uint256 a
    ) public returns (uint256) {
        uint256 value;
        assembly {
            tstore(slot, a)
        }
        value = 1;
        assembly {
            value := tload(slot)
        }
        return value - a;
    }
}
