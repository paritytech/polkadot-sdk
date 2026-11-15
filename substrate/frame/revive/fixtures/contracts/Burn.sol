// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract Burn {
	/// Burn `amount` while being deployed.
	constructor(uint256 amount) payable {
		if (amount != 0) {
			ISystem(SYSTEM_ADDR).burn(amount);
		}
	}

	function burn(uint256 amount) external payable {
		ISystem(SYSTEM_ADDR).burn(amount);
	}

	function burnThenRevert(uint256 amount) external {
		ISystem(SYSTEM_ADDR).burn(amount);
		revert("after burn");
	}

	/// Call `burnThenRevert` on this contract and report how it ended.
	function tryBurnThenRevert(
		uint256 amount
	) external returns (bool success, bytes memory output) {
		(success, output) = address(this).call(abi.encodeCall(this.burnThenRevert, (amount)));
	}

	function burnAll() external {
		ISystem(SYSTEM_ADDR).burn(address(this).balance);
	}

	function tryBurn(uint256 amount) external returns (bool success, bytes memory output) {
		(success, output) = SYSTEM_ADDR.call(abi.encodeCall(ISystem.burn, (amount)));
	}

	function tryBurnWithValue(
		uint256 amount,
		uint256 value
	) external returns (bool success, bytes memory output) {
		(success, output) = SYSTEM_ADDR.call{value: value}(abi.encodeCall(ISystem.burn, (amount)));
	}

	function tryBurnStatic(
		uint256 amount
	) external view returns (bool success, bytes memory output) {
		(success, output) = SYSTEM_ADDR.staticcall(abi.encodeCall(ISystem.burn, (amount)));
	}

	/// Delegate call the pre-compile itself.
	function tryBurnDelegate(uint256 amount) external returns (bool success, bytes memory output) {
		(success, output) = SYSTEM_ADDR.delegatecall(abi.encodeCall(ISystem.burn, (amount)));
	}

	/// Run `burn` of `implementation` as this contract, like a proxy does.
	function burnAsProxy(address implementation, uint256 amount) external {
		(bool success, bytes memory output) = implementation.delegatecall(
			abi.encodeCall(this.burn, (amount))
		);
		if (!success) {
			assembly {
				revert(add(output, 0x20), mload(output))
			}
		}
	}
}
