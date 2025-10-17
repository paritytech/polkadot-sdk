// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @dev The on-chain address of the Referenda precompile.
address constant REFERENDA_PRECOMPILE_ADDRESS = address(0xB0000);

/// @title Referenda Precompile Interface
/// @dev Exposes a lower-level interface to the Referenda pallet functionality.
interface IReferenda {
	/// @notice When the referendum should be enacted.
	enum Timing {
		/// @custom:variant Enact at specific block number
		AtBlock,
		/// @custom:variant Enact after N blocks from approval
		AfterBlock
	}

	/// @notice The origin of a referendum submission.
	/// @dev This is an encoded representation of the origin type in Polkadot/Kusama governance. For extension, new types can be added as needed at the end of the enum.
	enum GovernanceOrigin {
		/// @custom:variant The origin with the highest level of privileges.
		Root,
		/// @custom:variant Origin commanded by the Fellowship whitelist some hash of a call and allow the call to be dispatched with the root origin.
		WhitelistedCaller,
		/// @custom:variant The Wish For Change track serves as a medium for gathering consensus on a proposed change.
		WishForChange,
		/// @custom:variant The origin for canceling slashes. This origin has the privilege to execute calls from the staking pallet and the Election Provider Multiphase Pallet.
		StakingAdmin,
		/// @custom:variant The origin for spending funds from the treasury. This origin has the privilege to execute calls from the Treasury pallet.
		Treasurer,
		/// @custom:variant This origin can force slot leases. This origin has the privilege to execute calls from the Slots pallet.
		LeaseAdmin,
		/// @custom:variant The origin for managing the composition of the fellowship.
		FellowshipAdmin,
		/// @custom:variant The origin managing the registrar and permissioned HRMP channel operations.
		GeneralAdmin,
		/// @custom:variant Origin for starting auctions.
		AuctionAdmin,
		/// @custom:variant This origin can cancel referenda.
		ReferendumCanceller,
		/// @custom:variant The origin can cancel an ongoing referendum and slash the deposits.
		ReferendumKiller,
		/// @custom:variant Origin for submitting small tips.
		SmallTipper,
		/// @custom:variant Origin for submitting big tips.
		BigTipper,
		/// @custom:variant Origin able to spend small amounts from the treasury.
		SmallSpender,
		/// @custom:variant Origin able to spend medium amounts from the treasury.
		MediumSpender,
		/// @custom:variant Origin able to spend large amounts from the treasury.
		BigSpender
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
	/// @param origin The origin of the proposal.
	/// @param hash The hash of the referendum info to be looked up.
	/// @param preimageLength The length of the preimage in bytes.
	/// @param timing When the referendum should be enacted as defined in the `Timing` enum.
	/// @param enactmentMoment If `timing` is `AtBlock`, the block number for enactment. If `timing` is `AfterBlock`, the number of blocks after which to enact.
	/// @return referendumIndex The index of the newly created referendum.
	function submitLookup(
		GovernanceOrigin origin,
		bytes32 hash,
		uint32 preimageLength,
		Timing timing,
		uint32 enactmentMoment
	) external returns (uint32 referendumIndex);

	/// @notice Submit a referendum inline (for small proposals).
	/// @param origin The origin of the proposal.
	/// @param proposal The proposal call data to be submitted inline.
	/// @param timing When the referendum should be enacted as defined in the `Timing` enum.
	/// @param enactmentMoment If `timing` is `AtBlock`, the block number for enactment. If `timing` is `AfterBlock`, the number of blocks after which to enact.
	/// @return referendumIndex The index of the newly created referendum.
	function submitInline(
		GovernanceOrigin origin,
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

	/// @notice Get voting tally for an ongoing referendum
	/// @param referendumIndex The index of the referendum to query.
	/// @return exists Whether referendum exists and is ongoing
	/// @return ayes Aye votes (post-conviction if using conviction voting)
	/// @return nays Nay votes (post-conviction if using conviction voting)
	function getReferendumTally(
		uint32 referendumIndex
	) external view returns (bool exists, uint128 ayes, uint128 nays);

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
