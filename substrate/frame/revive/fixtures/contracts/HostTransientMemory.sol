// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract HostTransientMemory {
    function transientMemoryTest(uint64 slot, uint64 a) public returns (uint64) {
        uint256 value;
        assembly {
            tstore(slot, a)
        }
        value = 1;
        assembly {
            value := tload(slot)
        }
        return uint64(value - a);
    }
}
