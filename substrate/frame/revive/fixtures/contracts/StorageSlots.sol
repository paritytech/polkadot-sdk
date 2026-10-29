// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/// Writes and clears a range of storage slots.
contract StorageSlots {
	mapping(uint256 => uint256) private slots;

	/// Writes a non-zero value to the slots `from` to `to - 1`.
	function write(uint256 from, uint256 to) external {
		for (uint256 i = from; i < to; i++) {
			slots[i] = i + 1;
		}
	}

	/// Clears the slots `from` to `to - 1`.
	function clear(uint256 from, uint256 to) external {
		for (uint256 i = from; i < to; i++) {
			delete slots[i];
		}
	}
}

/// Deploys `StorageSlots`, so that it can be instantiated from a call by any origin.
contract StorageSlotsFactory {
	/// Deploys a new `StorageSlots` and returns its address.
	function deploy() external returns (address) {
		return address(new StorageSlots());
	}
}
