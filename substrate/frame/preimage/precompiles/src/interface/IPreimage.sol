// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @dev The on-chain address of the Preimage precompile.
address constant PREIMAGE_PRECOMPILE_ADDRESS = address(0xD0000);

/// @title `pallet_preimage` Precompile Interface
/// @notice An interface for interacting with `pallet_preimage`.
/// It forwards calls directly to the corresponding dispatchable functions.
interface IPreimage {
	/// @notice Register a preimage on-chain.
	/// @dev This transaction is free when called by a manager or when `preImage` was previously requested.
	/// @param preImage The preimage to be registered on-chain.
	/// @custom:reverts If `preImage` is larger than the allowed `CALLDATA_BYTES`.
	/// @custom:reverts If `preImage` is already noted when called by a non-manager.
	/// @custom:reverts If caller has unsufficient funds for deposit.
	/// @return hash The hash of the preimage
	function notePreimage(bytes memory preImage) external returns (bytes32 hash);

	/// @notice Clear an unrequested preimage from storage.
	/// @dev If succesfull, will return any held deposits.
	/// @param hash The preimage to be cleared from storage
	/// @custom:reverts If there is no preimage noted under `hash`.
	/// @custom:reverts If a non-manager attemps to unnote a preimage they did not note.
	function unnotePreimage(bytes32 hash) external;
}
