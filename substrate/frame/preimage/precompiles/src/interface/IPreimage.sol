// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @dev The on-chain address of the Preimage precompile.
address constant PREIMAGE_PRECOMPILE_ADDRESS = address(0xD0000);

interface IPreimage {
	/// @dev Register a preimage on-chain.
	/// @param preImage The preimage to be registered on-chain
	/// @return hash The hash of the preimage
	function notePreimage(bytes calldata preImage) external returns (bytes32 hash);

	/// @dev Clear an unrequested preimage from storage.
	/// @param hash The preimage to be cleared from storage
	function unnotePreimage(bytes32 hash) external;
}
