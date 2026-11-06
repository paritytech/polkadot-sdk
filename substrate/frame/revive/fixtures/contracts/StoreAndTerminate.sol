// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

contract StoreAndTerminate {
	uint256 public value;

	// Only a contract created in the same call stack is deleted, so the constructor is the one
	// place a top level deploy can terminate itself.
	constructor() payable {
		if (msg.value > 0) {
			selfdestruct(payable(msg.sender));
		}
	}

	function store(uint256 newValue) external {
		value = newValue;
	}
}
