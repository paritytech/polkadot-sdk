// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @dev The on-chain address of the Referenda precompile.
address constant REFERENDA_PRECOMPILE_ADDRESS = address(0xB0000);

/// @title Referenda Precompile Interface
/// @notice A low-level interface for interacting with `pallet_referenda`.
/// It forwards calls directly to the corresponding dispatchable functions,
/// providing access to referendum submission and management.
/// @dev Documentation:
/// @dev - OpenGov: https://wiki.polkadot.com/learn/learn-polkadot-opengov
/// @dev - SCALE codec: https://docs.polkadot.com/polkadot-protocol/parachain-basics/data-encoding
interface IReferenda {
	/// @notice When the referendum should be enacted.
	enum Timing {
		/// @custom:variant Enact at specific block number
		AtBlock,
		/// @custom:variant Enact after N blocks from approval
		AfterBlock
	}

	/// @notice Information about a referendum status.
	enum ReferendumStatus {
		/// @custom:variant /// Referendum has been submitted and has substatus defined by `OngoingPhase`.
		Ongoing,
		/// @custom:variant Referendum finished with approval. Submission deposit is held.
		Approved,
		/// @custom:variant Referendum finished with rejection. Submission deposit is held.
		Rejected,
		/// @custom:variant Referendum finished with cancellation. Submission deposit is held.
		Cancelled,
		/// @custom:variant Referendum finished and was never decided. Submission deposit is held.
		TimedOut,
		/// @custom:variant Referendum finished with a kill.
		Killed
	}

	/// @notice Sub-phases of an ongoing referendum.
	enum OngoingPhase {
		/// @custom:variant Referendum is waiting for decision deposit to be placed
		AwaitingDeposit,
		/// @custom:variant Decision deposit placed, preparing
		Preparing,
		/// @custom:variant Ready but waiting for track space
		Queued,
		/// @custom:variant Active voting period
		Deciding,
		/// @custom:variant Passing, in confirmation period
		Confirming
	}

	/// @notice Submit a referendum via preimage lookup (for large proposals).
	/// @dev Requires prior call to `pallet_preimage::note_preimage()`
	/// @param origin The SCALE-encoded `PalletsOrigin` origin of the proposal.
	/// @param hash The hash of the referendum info to be looked up.
	/// @param preimageLength The length of the preimage in bytes.
	/// @param timing When the referendum should be enacted as defined in the `Timing` enum.
	/// @param enactmentMoment If `timing` is `AtBlock`, the block number for enactment. If `timing` is `AfterBlock`, the number of blocks after which to enact.
	/// @return referendumIndex The index of the newly created referendum.
	function submitLookup(
		bytes calldata origin,
		bytes32 hash,
		uint32 preimageLength,
		Timing timing,
		uint32 enactmentMoment
	) external returns (uint32 referendumIndex);

	/// @notice Submit a referendum inline (for small proposals).
	/// @param origin The SCALE-encoded `PalletsOrigin` origin of the proposal.
	/// @param proposal The proposal call data to be submitted inline.
	/// @param timing When the referendum should be enacted as defined in the `Timing` enum.
	/// @param enactmentMoment If `timing` is `AtBlock`, the block number for enactment. If `timing` is `AfterBlock`, the number of blocks after which to enact.
	/// @return referendumIndex The index of the newly created referendum.
	function submitInline(
		bytes calldata origin,
		bytes calldata proposal,
		Timing timing,
		uint32 enactmentMoment
	) external returns (uint32 referendumIndex);

	/// @notice Place the decision deposit for a referendum.
	/// @param referendumIndex The index of the referendum for which to place the deposit.
	function placeDecisionDeposit(uint32 referendumIndex) external;

	/// @notice Set metadata for a referendum. Only callable by the referendum submitter.
	/// @param referendumIndex The index of the referendum for which to set metadata.
	/// @param metadataHash The hash of the metadata to associate with the referendum.
	function setMetadata(uint32 referendumIndex, bytes32 metadataHash) external;

	/// @notice Clear metadata for a referendum and refund the metadata deposit.
	/// @param referendumIndex The index of the referendum for which to clear metadata.
	function clearMetadata(uint32 referendumIndex) external;

	/// @notice Refund the submission deposit for a referendum.
	/// @param referendumIndex The index of the referendum for which to refund the deposit.
	/// @return refundAmount The amount refunded to the submitter.
	function refundSubmissionDeposit(
		uint32 referendumIndex
	) external returns (uint128 refundAmount);

	/// @notice Refund the decision deposit for a referendum.
	/// @param referendumIndex The index of the referendum for which to refund the deposit.
	/// @return refundAmount The amount refunded to the depositor.
	function refundDecisionDeposit(uint32 referendumIndex) external returns (uint128 refundAmount);

	/// @notice Get comprehensive referendum information
	/// @param referendumIndex The index of the referendum to query.
	/// @return exists Whether the referendum exists
	/// @return status Current status as defined in the `ReferendumStatus` enum
	/// @return ongoingPhase Sub-phase if status is Ongoing as defined in the `OngoingPhase` enum
	/// @return trackId The governance track ID
	/// @return proposalHash Hash of the proposal
	/// @return submissionDeposit Submission deposit amount
	/// @return decisionDeposit Decision deposit amount
	/// @return enactmentBlock Block number for execution (if approved)
	/// @return submissionBlock Block when referendum was submitted
	function getReferendumInfo(
		uint32 referendumIndex
	)
		external
		view
		returns (
			bool exists,
			ReferendumStatus status,
			OngoingPhase ongoingPhase,
			uint16 trackId,
			bytes32 proposalHash,
			uint128 submissionDeposit,
			uint128 decisionDeposit,
			uint32 enactmentBlock,
			uint32 submissionBlock
		);

	/// @notice Check if a referendum would pass if ended now
	/// @param referendumIndex The referendum index
	/// @return exists Whether the referendum exists
	/// @return passing Whether the referendum would pass if ended now
	function isReferendumPassing(
		uint32 referendumIndex
	) external view returns (bool exists, bool passing);

	/// @notice Get the submission deposit amount required for submitting a referendum
	/// @return The submission deposit amount
	function submissionDeposit() external view returns (uint128);
}
