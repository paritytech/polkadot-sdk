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

    function createThenCall(uint256 value) external returns (address child) {
        child = address(new ReceivingChild());
        child.call{value: value}("");
    }

    function callChild(address child) external returns (address) {
        child.call{value: 0}("");
        return child;
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
