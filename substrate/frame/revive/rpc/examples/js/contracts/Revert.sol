// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RevertExample {
    constructor() {
    }

    function do_revert() public {
		revert("revert message");
    }
}
