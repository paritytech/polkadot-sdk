// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

contract NestedDeployer {
    function deployChild() external returns (address) {
        return address(new NestedChild());
    }
}

contract NestedChild {
    uint256 public state;

    constructor() {
        state = 42;
    }
}
