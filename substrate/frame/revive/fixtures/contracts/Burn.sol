// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/ISystem.sol";

contract Burn {
	/// Written before and after a burn, so that the burn happens between two storage writes.
	uint256 public sold = 1;

	/// Burn `amount` of the value sent along with the deployment.
	constructor(uint256 amount) payable {
		if (amount != 0) {
			ISystem(SYSTEM_ADDR).burn(amount);
		}
	}

	/// Burn exactly the value sent along with this call.
	function buy() external payable {
		sold += 1;
		ISystem(SYSTEM_ADDR).burn(msg.value);
		sold += 1;
	}

	function burn(uint256 amount) external {
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

	/// Call `burn` of `other`, which burns from `other`.
	function burnVia(address other, uint256 amount) external {
		Burn(other).burn(amount);
	}

	/// Static call `burn` of this contract. The nested frame makes a regular call to the
	/// pre-compile while being read-only.
	function tryBurnNestedStatic(
		uint256 amount
	) external view returns (bool success, bytes memory output) {
		(success, output) = address(this).staticcall(abi.encodeCall(this.burn, (amount)));
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
