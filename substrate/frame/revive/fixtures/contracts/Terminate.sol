// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract Terminate {
	uint8 public constant METHOD_PRECOMPILE = 0;
	uint8 public constant METHOD_DELEGATE_CALL = 1;
	uint8 public constant METHOD_SYSCALL = 2;
    receive() external payable {}
	constructor(bool skip, uint8 method, address beneficiary) payable {
		if (skip) {
			return;
		}
		_terminate(method, beneficiary);
	}

	function terminate(uint8 method, address beneficiary) external {
		_terminate(method, beneficiary);
	}

	function indirectDelegateTerminate(address beneficiary) external {
		bytes memory data = abi.encodeWithSelector(this.terminate.selector, METHOD_PRECOMPILE, beneficiary);
		(bool success, bytes memory returnData) = address(this).delegatecall(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}
	/// Call terminate and forward any revert.
    /// Internal dispatcher: executes termination by
    /// - delegatecall (METHOD_DELEGATE_CALL) to system precompile
    /// - direct call (METHOD_PRECOMPILE) to system precompile
    /// - selfdestruct (METHOD_SYSCALL) sending balance to beneficiary
	function _terminate(uint8 method, address beneficiary) private {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = (false, "");

		if (method == METHOD_DELEGATE_CALL) {
			(success, returnData) = SYSTEM_ADDR.delegatecall(data);
		} else if (method == METHOD_PRECOMPILE) {
			(success, returnData) = SYSTEM_ADDR.call(data);
		} else if (method == METHOD_SYSCALL) {
			assembly {
				selfdestruct(beneficiary)
			}
			// selfdestruct halts execution, so if we reach here, something went wrong.
			revert("selfdestruct opcode returned");
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

