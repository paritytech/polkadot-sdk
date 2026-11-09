// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "@revive/ISystem.sol";

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
	function clear(uint256 from, uint256 to) public {
		for (uint256 i = from; i < to; i++) {
			delete slots[i];
		}
	}

	/// Clears the slots `from` to `to - 1` and then terminates this contract through the system
	/// precompile, sending its balance to `beneficiary`.
	function clearAndTerminate(uint256 from, uint256 to, address beneficiary) external {
		clear(from, to);
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
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
