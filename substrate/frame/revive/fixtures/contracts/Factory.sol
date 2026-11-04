// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

/**
 * @title Factory
 * @dev Creates different kinds of contracts and optionally calls them.
 */
contract Factory {
    function noop() external {}

    function create() external returns (address child) {
        child = address(new ReceivingChild());
    }

    function createAndFund() external payable returns (address child) {
        child = address(new ReceivingChild());
        (bool success, ) = child.call{value: msg.value}("");
        require(success, "funding failed");
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

    function createGasBurning() external {
        try new GasBurningChild() {} catch {}
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

/**
 * @title GasBurningChild
 * @dev Uses up all the gas it gets in the constructor.
 */
contract GasBurningChild {
    constructor() {
        assembly {
            invalid()
        }
    }
}
