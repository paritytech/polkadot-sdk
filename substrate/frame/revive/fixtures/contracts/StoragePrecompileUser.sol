// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "@revive/IStorage.sol";

/// Takes and clears its own `value` slot through the storage precompile.
contract StoragePrecompileUser {
	uint256 public value;

	function set(uint256 v) external {
		value = v;
	}

	function take() public returns (bytes memory) {
		bytes memory ret = callStorage(abi.encodeCall(IStorage.takeStorage, (0, true, valueKey())));
		return abi.decode(ret, (bytes));
	}

	function clear() public returns (bool containedKey, uint256 valueLen) {
		bytes memory ret = callStorage(abi.encodeCall(IStorage.clearStorage, (0, true, valueKey())));
		return abi.decode(ret, (bool, uint256));
	}

	/// Calls `take` on itself through a static call and returns the raw outcome.
	function takeInStaticCall() external view returns (bool ok, bytes memory ret) {
		return address(this).staticcall(abi.encodeCall(this.take, ()));
	}

	/// Calls `clear` on itself through a static call and returns the raw outcome.
	function clearInStaticCall() external view returns (bool ok, bytes memory ret) {
		return address(this).staticcall(abi.encodeCall(this.clear, ()));
	}

	/// `value` is the only state variable, so it lives in slot 0.
	function valueKey() private pure returns (bytes memory) {
		return abi.encodePacked(bytes32(0));
	}

	function callStorage(bytes memory data) private returns (bytes memory) {
		(bool ok, bytes memory ret) = STORAGE_ADDR.delegatecall(data);
		if (!ok) {
			assembly {
				revert(add(ret, 0x20), mload(ret))
			}
		}
		return ret;
	}
}
