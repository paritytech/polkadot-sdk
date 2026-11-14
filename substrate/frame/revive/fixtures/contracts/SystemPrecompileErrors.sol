// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

/// Calls the system precompile in ways that must revert with a reason.
contract SystemPrecompileErrors {
	constructor() payable {}

	function terminate(address beneficiary) public {
		(bool ok, bytes memory ret) =
			SYSTEM_ADDR.call(abi.encodeCall(ISystem.terminate, (beneficiary)));
		if (!ok) {
			assembly {
				revert(add(ret, 0x20), mload(ret))
			}
		}
	}

	/// Calls `terminate` on itself through a static call and returns the raw outcome.
	function terminateInStaticCall(address beneficiary)
		external
		view
		returns (bool ok, bytes memory ret)
	{
		return address(this).staticcall(abi.encodeCall(this.terminate, (beneficiary)));
	}

	/// Returns the raw outcome of `ecdsaToEthAddress`.
	function ecdsaToEthAddress(uint8[33] calldata publicKey)
		external
		view
		returns (bool ok, bytes memory ret)
	{
		return SYSTEM_ADDR.staticcall(abi.encodeCall(ISystem.ecdsaToEthAddress, (publicKey)));
	}
}
