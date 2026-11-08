// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/// Calls another contract and then writes storage of its own, so that a single transaction
/// combines the storage changes of two contracts.
contract CallThenWrite {
	mapping(uint256 => uint256) private slots;

	/// Calls `target` with `data`, reverting if that call fails, then writes `count` slots.
	function callThenWrite(address target, bytes calldata data, uint256 count) external {
		(bool success, bytes memory returnData) = target.call(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
		for (uint256 i = 0; i < count; i++) {
			slots[i] = i + 1;
		}
	}
}
