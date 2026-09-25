// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title Factory
 * @dev Creates different kinds of contracts and optionally calls them.
 */
contract Factory {
    function noop() external {}

    function create() external {
        new ReceivingChild();
    }

    function createThenCall(uint256 value) external {
        address(new ReceivingChild()).call{value: value}("");
    }

    function createReverting() external {
        try new RevertingChild() {} catch {}
    }
}

/**
 * @title ReceivingChild
 * @dev Implements only `receive`.
 */
contract ReceivingChild {
    receive() external payable {}
}

/**
 * @title RevertingChild
 * @dev Reverts in the constructor.
 */
contract RevertingChild {
    constructor() {
        revert();
    }
}
