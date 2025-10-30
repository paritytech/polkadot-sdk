// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract Terminate {
	constructor(bool skip) {
		if (skip) {
			return;
		}
		_terminate(false);
	}

	function terminate() external {
		_terminate(false);
	}

	function delegateTerminate() external {
		_terminate(true);
	}

	function indirectDelegateTerminate() external {
		bytes memory data = abi.encodeWithSelector(this.terminate.selector);
		(bool success, bytes memory returnData) = address(this).delegatecall(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}

	// Call terminate and forward any revert
	function _terminate(bool delegate) private {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, address(0));
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
