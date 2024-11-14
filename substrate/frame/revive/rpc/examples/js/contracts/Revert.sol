// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RevertExample {
    constructor() {
    }

    function doRevert() public {
		revert("revert message");
    }
}
