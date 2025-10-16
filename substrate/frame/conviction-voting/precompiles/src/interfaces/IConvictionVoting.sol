// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @dev The on-chain address of the Conviction Voting precompile.
address constant CONVICTION_VOTING_PRECOMPILE_ADDRESS = address(0xC0000);

/// @title ConvictionVoting Interface
interface IConvictionVoting {
	/// @notice A value denoting the strength of conviction of a vote.
	enum Conviction {
		/// @custom:variant 0.1x votes, unlocked.
		None,
		/// @custom:variant 1x votes, locked for an enactment period following a successful vote.
		Locked1x,
		/// @custom:variant 2x votes, locked for 2x enactment periods following a successful vote.
		Locked2x,
		/// @custom:variant 3x votes, locked for 4x...
		Locked3x,
		/// @custom:variant 4x votes, locked for 8x...
		Locked4x,
		/// @custom:variant 5x votes, locked for 16x...
		Locked5x,
		/// @custom:variant 6x votes, locked for 32x...
		Locked6x
	}

	/// @notice The type of vote cast.
	enum VotingType {
		/// @custom:variant A standard vote, one-way (approve or reject) with a given amount of conviction.
		Standard,
		/// @custom:variant A split vote with balances given for both ways, and with no conviction.
		Split,
		/// @custom:variant A split vote with balances given for both ways as well as abstentions, and with no conviction.
		SplitAbstain
	}

	/// @notice Cast a standard vote (aye or nay) with conviction.
	/// @param referendumIndex The index of the referendum to vote on.
	/// @param aye True for approving, false for rejecting.
	/// @param conviction Conviction level as defined in the `Conviction` enum.
	/// @param balance The amount of tokens to vote with.
	function voteStandard(
		uint32 referendumIndex,
		bool aye,
		Conviction conviction,
		uint128 balance
	) external;

	/// @notice Cast a split vote with explicit aye and nay balances, no conviction/lock applied.
	/// @param referendumIndex The index of the referendum to vote on.
	/// @param ayeAmount Balance allocated to aye.
	/// @param nayAmount Balance allocated to nay.
	function voteSplit(uint32 referendumIndex, uint128 ayeAmount, uint128 nayAmount) external;

	/// @notice Cast a split vote with explicit aye, nay and abstain balances, no conviction/lock applied.
	/// @param referendumIndex The index of the referendum to vote on.
	/// @param ayeAmount Balance allocated to aye.
	/// @param nayAmount Balance allocated to nay.
	/// @param abstainAmount Balance allocated to abstain.
	function voteSplitAbstain(
		uint32 referendumIndex,
		uint128 ayeAmount,
		uint128 nayAmount,
		uint128 abstainAmount
	) external;

	/// @notice Remove a vote from a referendum.
	/// @param trackId The governance track identifier.
	/// @param referendumIndex The referendum index.
	function removeVote(uint16 trackId, uint32 referendumIndex) external;

	/// @notice Delegate voting power to another account within a specific governance track.
	/// @dev Applies the sender’s balance with the specified conviction multiplier.
	/// @param trackId The governance track identifier.
	/// @param to The substrate account to which voting power is delegated (32-byte bytes32). See https://docs.polkadot.com/polkadot-protocol/smart-contract-basics/accounts/#ethereum-to-polkadot-mapping.
	/// @param conviction Conviction level as defined in the `Conviction` enum.
	function delegate(uint16 trackId, bytes32 to, Conviction conviction, uint128 balance) external;

	/// @notice Remove any existing delegation within a governance track.
	/// @param trackId The governance track identifier.
	function undelegate(uint16 trackId) external;

	/// @notice Get the current vote details for specific referendum of an account in a governance track
	/// @param who The account to query
	/// @param trackId The governance track to query
	/// @param referendumIndex The referendum index to query
	/// @return exists Whether a vote exists
	/// @return votingType The type of vote as defined in the `VotingType` enum.
	/// @return aye True if a standard vote is aye, false if nay. False for split and split-abstain votes.
	/// @return ayeAmount The amount of tokens voting aye (pre-conviction). 0 for standard nay votes.
	/// @return nayAmount The amount of tokens voting nay (pre-conviction). 0 for standard aye votes.
	/// @return abstainAmount The amount of tokens voting abstain (pre-conviction). 0 for standard and split votes.
	/// @return conviction The conviction level applied to the vote as defined in the `Conviction` enum. Not applicable for split and split-abstain votes.
	function getVoting(
		address who,
		uint16 trackId,
		uint32 referendumIndex
	)
		external
		view
		returns (
			bool exists,
			VotingType votingType,
			bool aye,
			uint128 ayeAmount,
			uint128 nayAmount,
			uint128 abstainAmount,
			Conviction conviction
		);

	/// @notice Get the current delegation details for an account in a governance track.
	/// @dev Returns zero values if no delegation.
	/// @param who The account to query
	/// @param trackId The governance track to query
	/// @return target The account to which voting power is delegated (32-byte bytes32). Is 0 when there is no delegation. See https://docs.polkadot.com/polkadot-protocol/smart-contract-basics/accounts/#polkadot-to-ethereum-mapping.
	/// @return balance The amount of tokens delegated (pre-conviction).
	/// @return conviction The conviction level applied to the delegation as defined in the `Conviction` enum.
	function getDelegation(
		address who,
		uint16 trackId
	) external view returns (bytes32 target, uint128 balance, Conviction conviction);
}
