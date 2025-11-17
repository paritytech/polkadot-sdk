// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract Terminate {
	constructor(bool skip, address beneficiary) {
		if (skip) {
			return;
		}
		_terminate(false, beneficiary);
	}

	function terminate(address beneficiary) external {
		_terminate(false, beneficiary);
	}

	function delegateTerminate(address beneficiary) external {
		_terminate(true, beneficiary);
	}

	function indirectDelegateTerminate(address beneficiary) external {
		bytes memory data = abi.encodeWithSelector(this.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = address(this).delegatecall(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}

	// Call terminate and forward any revert
	function _terminate(bool delegate, address beneficiary) private {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = (false, "");

		if (delegate) {
			(success, returnData) = SYSTEM_ADDR.delegatecall(data);
		} else {
			(success, returnData) = SYSTEM_ADDR.call(data);
		}

		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}
}
