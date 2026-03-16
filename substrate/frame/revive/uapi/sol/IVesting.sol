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

	/// Unlock any vested funds of another account.
	///
	/// The `target` account must have funds still locked under the vesting pallet.
	/// On success the vesting lock is reduced in line with the amount "vested" so far.
	/// The caller pays the fee but the vesting schedule of `target` is updated.
	///
	/// Reverts if `target` has no vesting schedule or if the origin is not signed.
	function vestOther(address target) external;

	/// Returns the amount of funds still locked (to be vested) for the caller.
	///
	/// The returned value is in native (Substrate) denomination.
	/// Returns 0 in two cases: the caller has no vesting schedule, or the caller
	/// has a schedule but all funds are already unlocked (fully vested). Both cases
	/// mean there is nothing left to vest; calling vest() in either case will revert.
	function vestingBalance() external view returns (uint256);
}
