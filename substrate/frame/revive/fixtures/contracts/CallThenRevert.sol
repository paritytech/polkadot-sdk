// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title CallThenRevert
 * @dev Calls a target, then reverts.
 */
contract CallThenRevert {
    fallback() external {}

    function callThenRevert(address target) external {
        target.call("");
        revert();
    }
}
