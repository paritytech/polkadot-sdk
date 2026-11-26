// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import { ClearableContributorStorage } from "./ClearableContributorStorage.sol";

/// Test fixture that creates `ClearableContributorStorage` contracts with `CREATE2`.
///
/// `createGrowAndTerminate` creates the contract, writes a storage slot to it and terminates it
/// in the same call, which leaves a terminated contract with a non-empty child trie at the
/// address. `create` creates the contract with the same salt again at that address.
contract ClearableContributorFactory {
	function createGrowAndTerminate(bytes32 salt, address beneficiary) external returns (address) {
		ClearableContributorStorage created = new ClearableContributorStorage{salt: salt}();
		created.growStorage();
		created.terminate(beneficiary);
		return address(created);
	}

	function create(bytes32 salt) external returns (address) {
		return address(new ClearableContributorStorage{salt: salt}());
	}
}
