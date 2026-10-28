// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

/// Deploys `RedeployChild` with CREATE2 at a fixed salt. A factory re-created at the same address
/// therefore deploys its child at the same address again. Both contracts terminate themselves
/// through the system precompile, so termination works across transactions.
contract RedeployFactory {
	function deployChild() external returns (address) {
		return address(new RedeployChild{salt: bytes32(0)}());
	}

	function terminate(address beneficiary) external {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}
}

/// Holds one storage value and terminates itself on request.
contract RedeployChild {
	uint256 public value;

	function setValue(uint256 newValue) external {
		value = newValue;
	}

	function terminate(address beneficiary) external {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}
}
