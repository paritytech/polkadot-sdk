// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

address constant VESTING_ADDR = 0x0000000000000000000000000000000000000902;

interface IVesting {
	/// Unlock any vested funds of the caller account.
	///
	/// The caller must have funds still locked under the vesting pallet.
	/// On success the vesting lock is reduced in line with the amount "vested" so far.
	///
	/// Reverts if the caller has no vesting schedule or if the origin is not signed.
	function vest() external;
}
