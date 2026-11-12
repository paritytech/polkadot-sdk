// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// Storage that any caller may write and clear, so that one account pays the deposit for slots
/// that another account later removes.
contract SharedStorage {
	mapping(uint256 => uint256) private slots;

	function write(uint256 from, uint256 to) external {
		for (uint256 i = from; i < to; i++) {
			slots[i] = i + 1;
		}
	}

	function clear(uint256 from, uint256 to) external {
		for (uint256 i = from; i < to; i++) {
			delete slots[i];
		}
	}
}
