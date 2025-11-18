// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract Terminate {
    receive() external payable {}
	constructor(bool skip, address beneficiary) payable {
		if (skip) {
			return;
		}
		_terminate(TerminateMethod.CALL, beneficiary);
	}

	function terminate(address beneficiary) external {
		_terminate(TerminateMethod.CALL, beneficiary);
	}

	function delegateTerminate(address beneficiary) external {
		_terminate(TerminateMethod.DELEGATE_CALL, beneficiary);
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
    enum TerminateMethod {
        CALL,           // 0
        DELEGATE_CALL,  // 1
        SYSCALL         // 2
    }
	// Call terminate and forward any revert
	function _terminate(TerminateMethod method, address beneficiary) private {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = (false, "");

		if (method == TerminateMethod.DELEGATE_CALL) {
			(success, returnData) = SYSTEM_ADDR.delegatecall(data);
		} else if (method == TerminateMethod.CALL) {
			(success, returnData) = SYSTEM_ADDR.call(data);
		} else if (method == TerminateMethod.SYSCALL) {
			assembly {
				selfdestruct(beneficiary)
			}
		} else {
			revert("Invalid TerminateMethod");
		}

		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}
}
