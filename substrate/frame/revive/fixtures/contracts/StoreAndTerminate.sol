// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract StoreAndTerminate {
	uint256 public value;

	function store(uint256 newValue) external {
		value = newValue;
	}

	function terminate(address beneficiary) external {
		(bool success, bytes memory returnData) =
			SYSTEM_ADDR.call(abi.encodeWithSelector(ISystem.terminate.selector, beneficiary));
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}
}
