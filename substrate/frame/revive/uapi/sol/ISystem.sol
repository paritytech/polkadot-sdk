// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

address constant SYSTEM_ADDR = 0x0000000000000000000000000000000000000900;

interface ISystem {
	/// Computes the BLAKE2 256-bit hash on the given input.
	function hashBlake256(bytes memory input) external pure returns (bytes32 digest);

	/// Computes the BLAKE2 128-bit hash on the given input.
	function hashBlake128(bytes memory input) external pure returns (bytes32 digest);

	/// Retrieve the account id for a specified `H160` address.
	///
	/// Calling this function on a native `H160` chain (`type AccountId = H160`)
	/// does not make sense, as it would just return the `address` that it was
	/// called with.
	///
	/// # Note
	///
	/// If no mapping exists for `addr`, the fallback account id will be returned.
	function toAccountId(address input) external view returns (bytes memory account_id);

	/// Checks whether the caller of the contract calling this function is the origin
	/// of the whole call stack.
	function callerIsOrigin() external view returns (bool);

	/// Checks whether the caller of the contract calling this function is root.
	///
	/// Note that only the origin of the call stack can be root. Hence this
	/// function returning `true` implies that the contract is being called by the origin.
	///
	/// A return value of `true` indicates that this contract is being called by a root origin,
	/// and `false` indicates that the caller is a signed origin.
	function callerIsRoot() external view returns (bool);

	/// Returns the minimum balance that is required for creating an account
	/// (the existential deposit).
	function minimumBalance() external view returns (uint);

	/// Returns the code hash of the caller.
	function ownCodeHash() external view returns (bytes32);

	/// Returns the amount of `Weight` left.
	function weightLeft() external view returns (uint64 refTime, uint64 proofSize);

	/// Terminate the calling contract of this function and send balance to `beneficiary`.
	/// This will revert if:
	/// - called from constructor
	/// - called from static context
	/// - called from delegate context
	/// - the contract introduced balance locks
	function terminate(address beneficiary) external;
}
