// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @title Referenda Interface
interface IReferenda {
	/// @notice When the referendum should be enacted.
	enum Timing {
		/// @custom:variant At specified block.
		AtBlock,
		/// @custom:variant After specified number of blocks.
		AfterBlock
	}

	/// @notice Information about a referendum status.
	enum ReferendumStatus {
		/// @custom:variant Referendum has been submitted but still waiting for decision deposit.
		Prepare,
		/// @custom:variant Referendum is being voted on, decision deposit has been placed.
		Deciding,
		/// @custom:variant Referendum is in the confirmation period after a successful vote.
		Confirming,
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

	/// @notice The origin of a referendum submission.
	/// @dev This is an encoded representation of the origin type in Polkadot/Kusama governance. For extension, new types can be added as needed at the end of the enum.
	enum GovernanceOrigin {
		Root,
		WhitelistedCaller,
		WishForChange,
		StakingAdmin,
		Treasurer,
		LeaseAdmin,
		FellowshipAdmin,
		GeneralAdmin,
		AuctionAdmin,
		ReferendumCanceller,
		ReferendumKiller,
		SmallTipper,
		BigTipper,
		SmallSpender,
		MediumSpender,
		BigSpender
	}

	/// @notice Information about a referendum.
	struct ReferendumInfo {
		/// @custom:property The governance track ID.
		uint16 trackId;
		/// @custom:property Lifecycle status as defined in the `ReferendumStatus` enum.
		ReferendumStatus status;
		/// @custom:property The origin of the referendum submission as defined in the `GovernanceOrigin` enum.
		GovernanceOrigin origin;
		/// @custom:property  The hash of the proposal call data.
		bytes32 callHash;
		/// @custom:property  When the referendum will be enacted if approved.
		uint32 enactmentBlock;
		/// @custom:property  When the referendum was submitted.
		uint32 submissionBlock;
		/// @custom:property  The block number when the referendum entered the Deciding phase. 0 if in Prepare phase.
		uint32 decidingSince;
		/// @custom:property  The block number when the referendum will exit the Confirming phase. 0 if not in Confirming phase.
		uint32 confirmingUntil;
		/// @custom:property  The amount of submission deposit held.
		uint128 submissionDeposit;
		/// @custom:property  The amount of decision deposit placed.
		uint128 decisionDeposit;
	}

	/// @notice Submit a referendum via preimage lookup (for large proposals).
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

	/// @notice Cancel an ongoing referendum. (requires the `ReferendumCanceller` origin in Polkadot/Kusama)
	/// @param referendumIndex The index of the referendum to cancel.
	function cancel(uint32 referendumIndex) external;

	/// @notice Kill an ongoing referendum and slash deposits. (requires the `ReferendumKiller` origin in Polkadot/Kusama)
	/// @param referendumIndex The index of the referendum to kill.
	function kill(uint32 referendumIndex) external;

	/// @notice Set metadata for a referendum. Only callable by the referendum submitter.
	/// @param referendumIndex The index of the referendum for which to set metadata.
	/// @param metadataHash The hash of the metadata to associate with the referendum.
	function setMetadata(uint32 referendumIndex, bytes32 metadataHash) external;

	/// @notice Clear metadata for a referendum and refund the metadata deposit.
	/// @param referendumIndex The index of the referendum for which to clear metadata.
	function clearMetadata(uint32 referendumIndex) external;

	/// @notice Get metadata hash for a referendum.
	/// @param referendumIndex The index of the referendum to query.
	function getMetadata(uint32 referendumIndex) external view returns (bytes32);

	/// @notice Check if a referendum is ongoing (in voting period).
	/// @param referendumIndex The referendum index
	/// @return exists Whether the referendum exists
	/// @return isOngoing Whether the referendum is ongoing (in either Prepare, Deciding, Confirming phase)
	function isOngoing(uint32 referendumIndex) external view returns (bool exists, bool isOngoing);

	/// @notice Check if a referendum would pass if ended now
	/// @param referendumIndex The referendum index
	/// @return exists Whether the referendum exists
	/// @return passing Whether the referendum would pass if ended now
	function isReferendumPassing(
		uint32 referendumIndex
	) external view returns (bool exists, bool passing);

	/// @notice Get information about a referendum
	/// @param referendumIndex The referendum index
	/// @return exists Whether the referendum exists
	/// @return The referendum information as a `ReferendumInfo` struct
	function getReferendumInfo(
		uint32 referendumIndex
	) external view returns (bool exists, ReferendumInfo memory);

	/// @notice Get voting tally for an ongoing referendum
	/// @param referendumIndex The referendum index
	/// @return inVoting Whether the referendum exists and is either in the Deciding or Confirming phase.
	/// @return ayes The number of aye votes, expressed in terms of post-conviction lock-vote.
	/// @return nays The number of nay votes, expressed in terms of post-conviction lock-vote.
	/// @return support The basic number of aye votes, expressed pre-conviction.
	function getReferendumTally(
		uint32 referendumIndex
	) external view returns (bool inVoting, uint128 ayes, uint128 nays, uint128 support);

	/// @notice Get track information
	/// @param track The track ID
	/// @return exists Whether the track exists
	/// @return maxDeciding Maximum concurrent deciding referenda
	/// @return decisionDeposit Required decision deposit
	/// @return preparePeriod Preparation period in blocks
	/// @return decisionPeriod Decision period in blocks
	/// @return confirmPeriod Confirmation period in blocks
	/// @return minEnactmentPeriod Minimum enactment period in blocks
	function getTrackInfo(
		uint16 track
	)
		external
		view
		returns (
			bool exists,
			uint32 maxDeciding,
			uint128 decisionDeposit,
			uint32 preparePeriod,
			uint32 decisionPeriod,
			uint32 confirmPeriod,
			uint32 minEnactmentPeriod
		);

	/// @notice Get the submission deposit amount required for submitting a referendum
	/// @return The submission deposit amount
	function submissionDeposit() external view returns (uint128);
}
