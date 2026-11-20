// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "@revive/ISystem.sol";

/// Test fixture for storage written under `Origin::Root` and removed by a signed origin
/// (and vice versa). Each `write` grows `n` fresh mapping slots starting at `from`, each
/// `clear` deletes them again.
contract RootStorage {
	mapping(uint64 => uint64) public slots;

	function write(uint64 from, uint64 n) external {
		for (uint64 i = 0; i < n; i++) {
			slots[from + i] = i + 1;
		}
	}

	function clear(uint64 from, uint64 n) external {
		for (uint64 i = 0; i < n; i++) {
			delete slots[from + i];
		}
	}

	/// Terminates through the system precompile, so the destruction happens at the end of the
	/// call stack.
	function destroy(address beneficiary) external {
		bytes memory data = abi.encodeWithSelector(ISystem.terminate.selector, beneficiary);
		(bool success, bytes memory returnData) = SYSTEM_ADDR.call(data);
		if (!success) {
			assembly {
				revert(add(returnData, 0x20), mload(returnData))
			}
		}
	}

	/// Writes and clears the same slots in two nested frames of one transaction, by calling
	/// back into this contract.
	function writeThenClear(uint64 from, uint64 n) external {
		this.write(from, n);
		this.clear(from, n);
	}
}

/// Creates `RootStorage` children through a nested CREATE.
contract RootStorageDeployer {
	/// Deploys a child and writes `n` slots into it in the same transaction.
	function deploy(uint64 n) external returns (address) {
		RootStorage child = new RootStorage();
		child.write(0, n);
		return address(child);
	}
}
