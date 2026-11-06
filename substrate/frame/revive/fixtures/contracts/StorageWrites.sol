// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title StorageWrites
 * @dev Applies a list of writes from its constructor or from a call. The list comes from the input
 * so that the optimizer can't merge repeated writes to a slot.
 */
contract StorageWrites {
    struct Write {
        uint256 slot;
        uint256 value;
    }

    constructor(Write[] memory writes) {
        for (uint256 i = 0; i < writes.length; i++) {
            store(writes[i]);
        }
    }

    function write(Write[] memory writes) external {
        for (uint256 i = 0; i < writes.length; i++) {
            store(writes[i]);
        }
    }

    function store(Write memory w) private {
        uint256 slot = w.slot;
        uint256 value = w.value;
        assembly {
            sstore(slot, value)
        }
    }
}
