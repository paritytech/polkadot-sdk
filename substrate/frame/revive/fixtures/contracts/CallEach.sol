// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title CallEach
 * @dev Calls each target with its data, in order, within one transaction. Results are ignored,
 * so a call that fails leaves the next ones running.
 */
contract CallEach {
    function callEach(address[] calldata targets, bytes[] calldata data) external {
        for (uint256 i = 0; i < targets.length; i++) {
            targets[i].call(data[i]);
        }
    }
}
