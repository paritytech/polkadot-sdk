# Governance Precompile — [#8366](https://github.com/paritytech/polkadot-sdk/issues/8366)

**Authors:** Eman Herawy & Lucas Grasso  
**Mentor:** Ankan Anurag — _PBA Bounty Hunters_

## Prelude

This issue is part of the OG Rust bounties, see [https://ogrust.com/].

## Summary

This proposal requests funding to implement an EVM precompile that exposes Polkadot's OpenGov functionality directly to smart contracts. This precompile will allow EVM developers to submit referenda, vote, delegate, and query governance state fully on-chain, without relying on Substrate RPCs.

By enabling direct OpenGov interactions from Solidity, we unlock hybrid EVM–Substrate governance dApps, reducing off-chain dependencies, improving UX, and expanding Polkadot's developer base.

**Development Approach:** We propose a **minimal-first strategy** — implementing 15 core functions that enable smart contracts to submit referenda, cast all vote types, manage delegation, and query governance state, while documenting the full 27-function interface for future extensions based on the learned lessons from the minimal implementation.

Development will proceed across **4 milestones** over **xxx weeks**, delivering a Solidity interface, Rust implementation, comprehensive tests, gas benchmarks, and documentation.

**Total requested bounty:** `xxxx DOT` (including implementation, testing, and documentation).

---

## Motivation

In view of the upcoming Asset Hub migration, creating a governance precompile will enable smart contracts (and their users) to directly participate in on-chain decision making.

By exposing key governance functions through a contract-friendly interface, DAOs and other contract applications can integrate natively with Polkadot's governance system, each with their own internal voting mechanisms.

---

## Deliverables

- Solidity Interface with its corresponding natspec documentation.
- `governance-precompiles` crate or `referenda-precompile` and `conviction-voting-precompile` crates , including:
  - Precompile implementation
  - Benchmarks
  - Tests
  - The documented interfaces mentioned above
- Smart contracts that demonstrate the precompile usage.

## Development Strategy: Minimal-First Approach

### Phase 1 (This Proposal): Minimal Interface - 15 Core Functions

We propose implementing **15 essential functions** that cover the essential smart contract governance operations:

**Core Capabilities:**

- Referendum submission (inline + lookup)
- All voting types (standard, split, split-abstain)
- Vote removal (essential for changing votes)
- Delegation management (delegate + undelegate)
- Essential queries (referendum info, vote details, tally, delegation status)

### Phase 2 (Future Consideration): Extended Interface - Additional 11 Functions

We have designed a **full interface with 27 functions** for advanced use cases. These can be added in future proposals based on community feedback and real-world usage:

**Extended Capabilities:**

- Advanced queries (track info, deciding status, locked balances)
- Deposit refunds (submission + decision)
- Vote cleanup operations (unlock, removeOtherVote)
- Additional metadata operations
- `kill` and `cancel` referenda may be added if deemed necessary, we do not include them in the full interface for now.

**Why Minimal-First?**

This approach prioritizes delivering core functionality quickly while minimizing risk through a smaller audit surface and incremental testing. By starting with essential features, we can gather real feedback and iterate based on actual usage patterns rather than assumptions, ensuring future extensions address genuine needs.
**The full interface is documented in the appendix for review and future planning.**

**Bounties and Treasury pallets:**
We do not consider these pallets for these two first phases, they might be included as a "Phase 3" in the future, or added to the "Phase 2" if the community thinks they are essential.

---

## Proposed Minimal Solidity Interface

### IMinimalConvictionVoting (8 functions)

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @title Minimal ConvictionVoting Interface
/// @dev Provides 8 core functions for voting and delegation
interface IMinimalConvictionVoting {
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
		bytes32 who,
		uint16 trackId
	) external view returns (bytes32 target, uint128 balance, Conviction conviction);
}
```

### IMinimalReferenda (7 functions)

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @title Minimal Referenda Interface
/// @dev Provides 7 core functions for referendum submission and queries
interface IMinimalReferenda {
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
	/// @return ayes Aye votes (post-conviction)
	/// @return nays Nay votes (post-conviction)
	/// @return support Aye votes (pre-conviction, for turnout calculation)
	function getReferendumTally(
		uint32 referendumIndex
	) external view returns (bool exists, uint128 ayes, uint128 nays, uint128 support);

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

```

### IMinimalGovernance (Combined Interface)

```solidity
/// @title Minimal Governance Precompile Interface
interface IMinimalGovernance is IMinimalReferenda, IMinimalConvictionVoting {
}
```

---

## Lifecycle Overview

```
┌─────────────────────┐
│   EVM Smart         │   (Contract calls precompile)
│   Contract          │
│                     │
│ - submitInline()    │ → Submit small referendum
│ - submitLookup()    │ → Submit large referendum (via preimage)
│ - voteStandard()    │ → Cast conviction vote
│ - delegate()        │ → Delegate voting power
└──────────┬──────────┘
           │
           │ Calls Precompile
           ▼
┌─────────────────────┐
│   EVM Precompile    │   (Rust implementation)
│   (Address: 0x...)  │
│                     │
│ - Type conversion   │ → Solidity types ↔ Substrate types
│ - Origin mapping    │ → GovernanceOrigin → RuntimeOrigin
│ - Deposit handling  │ → msg.value → Balance
└──────────┬──────────┘
           │
           │ Dispatches to Pallets
           ▼
┌──────────────────────────────────────────────────┐
│                                                  │
│  ┌─────────────────┐  ┌─────────────────┐      │
│  │ pallet_referenda│  │pallet_conviction│      │
│  │                 │  │   _voting       │      │
│  │ - submit()      │  │ - vote()        │      │
│  │ - place_deposit │  │ - delegate()    │      │
│  │ - tally()       │  │ - remove_vote() │      │
│  │ (Lifecycle mgmt)│  │ (Vote tracking) │      │
│  └─────────────────┘  └─────────────────┘      │
│                                                  │
└──────────────────────────────────────────────────┘
```

---

## Development Milestones

### Milestone 1: Referendum Submission & Core Queries

**Duration:** xxx weeks  
**Budget:** xxxx DOT

**Deliverables:**

1. `submitLookup()` - Submit referendum via preimage hash
2. `submitInline()` - Submit small proposals inline
3. `placeDecisionDeposit()` - Fund decision deposit
4. `getReferendumInfo()` - Get comprehensive referendum data
5. `submissionDeposit()` - Query required deposit amount

**Why First:**

- Creates the foundation - referenda that others can vote on
- Tests deposit handling and origin mapping early (highest risk areas)
- Enables immediate value: contracts can submit proposals

---

### Milestone 2: Voting Functions

**Duration:** xxx weeks  
**Budget:** xxxx DOT

**Deliverables:** 6. `voteStandard()` - Standard conviction voting 7. `voteSplit()` - Split voting (aye/nay without conviction) 8. `voteSplitAbstain()` - Three-way split voting 9. `getVoting()` - Query vote details

**Why Second:**

- Core user interaction with governance
- Natural progression: submit → vote
- Simpler than delegation (no recursive logic)
- Most frequently used functions

---

### Milestone 3: Delegation & Vote Management

**Duration:** xxx weeks  
**Budget:** xxxx DOT

**Deliverables:** 10. `delegate()` - Delegate voting power to another account 11. `undelegate()` - Remove delegation 12. `removeVote()` - Remove/change votes 13. `getDelegation()` - Query delegation status

**Why Third:**

- Advanced voting features
- Requires solid voting system foundation
- Delegation is complex (conviction, tracks, inheritance)
- Less frequently used than direct voting

---

### Milestone 4: Advanced Queries, Integration Testing & Documentation

**Duration:** xxx weeks  
**Budget:** xxxx DOT

**Deliverables:** 14. `getReferendumTally()` - Get vote counts (ayes, nays, support) 15. `isReferendumPassing()` - Check if referendum would pass

**Why Last:**

- Requires all previous milestones to be complete for integration testing
- Tally calculation is complex (needs track parameters, conviction math)
- Query-only operations (lowest risk)
- Allows comprehensive system-level testing
- Documentation reflects final implementation

---

## What's Excluded (Can Be Added in Phase 2)

The following functions are **intentionally excluded** from this proposal but documented for future consideration:

**Advanced Queries (4 functions):**

- `getLockedBalance()` - Query locked balance for a track
- `getTotalLocked()` - Query max locked balance across all tracks
- `getDecidingStatus()` - Get detailed deciding phase info
- `getTrackInfo()` - Query track configuration parameters

**Deposit Management (2 functions):**

- `refundSubmissionDeposit()` - Refund submission deposit (can use extrinsic)
- `refundDecisionDeposit()` - Refund decision deposit (can use extrinsic)

**Vote Cleanup (2 functions):**

- `unlock()` - Unlock expired voting locks (can use extrinsic)
- `removeOtherVote()` - Remove someone else's expired vote (governance operation)

**Metadata (3 functions):**

- `setMetadata()` - Set referendum metadata (IPFS hash)
- `clearMetadata()` - Clear referendum metadata
- `getMetadata()` - Query metadata hash
- **Note:** Metadata is UI-layer concern, rarely used by contracts

**Rationale for Exclusion:**

- These are **convenience functions**, not core governance operations
- All excluded functionality is available via direct extrinsic calls
- Contracts need **time-sensitive operations** in precompiles; everything else can wait
- Keeps audit surface small and focused
- Enables faster delivery and iteration

---

## Appendix: Full Interface Design

For transparency and future planning, we've designed the complete interface with **27 functions**. This is available for review but **not included in this proposal's scope**.

### Full Interface Function Count

| Category            | Minimal (Phase 1) | Extended (Phase 2) | Total  |
| ------------------- | ----------------- | ------------------ | ------ |
| **Submission**      | 3                 | 0                  | 3      |
| **Voting**          | 3                 | 0                  | 3      |
| **Vote Management** | 1                 | 1                  | 2      |
| **Delegation**      | 2                 | 0                  | 2      |
| **Queries**         | 6                 | 4                  | 10     |
| **Deposits**        | 1                 | 2                  | 3      |
| **Cleanup**         | 0                 | 1                  | 1      |
| **Metadata**        | 0                 | 3                  | 2      |
| **Total**           | **15**            | **12**             | **27** |

### Proposed Full Solidity Interface

### ConvictionVoting

```solidity

// SPDX-License-Identifier: MIT

pragma solidity ^0.8.30;

/// @title ConvictionVoting Interface
interface IConvictionVoting is IMinimalConvictionVoting {
   /// @notice Remove someone else's expired vote
   /// @param target The account whose vote to remove
   /// @param class The class of the poll
   /// @param pollIndex The poll index
   function removeOtherVote(
       address target,
       uint16 class,
       uint256 pollIndex
   ) external;

	/// @notice Unlock expired voting/delegation lock
	/// @param trackId The trackId/track ID to unlock
	/// @param target The account to unlock (can be yourself or others)
	function unlock(uint16 trackId, address target) external;

	/// @notice Get the locked balance for an account in a trackId
	/// @param who The account to query
	/// @param trackId The governance track to query
	/// @return The locked amount
	function getLockedBalance(address who, uint16 trackId) external view returns (uint128);

	/// @notice Get the maximum locked balance across all trackIds.
	/// @param who The account to query
	/// @return The total locked amount (max of all locks along governance tracks)
	function getTotalLocked(address who) external view returns (uint128);
}

```

### IReferenda

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @title Referenda Interface
interface IReferenda is IMinimalReferenda {
    /// @notice Refund the submission deposit for a referendum.
    /// @param referendumIndex The index of the referendum for which to refund the deposit.
    /// @return refundAmount The amount refunded to the submitter.
    function refundSubmissionDeposit(
        uint32 referendumIndex
    ) external returns (uint128 refundAmount);

    /// @notice Refund the decision deposit for a referendum.
    /// @param referendumIndex The index of the referendum for which to refund the deposit.
    /// @return refundAmount The amount refunded to the depositor.
    function refundDecisionDeposit(
        uint32 referendumIndex
    ) external returns (uint128 refundAmount);

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

    /// @notice Get deciding status for an ongoing referendum
    /// @param referendumIndex The referendum index
    /// @return exists Whether the referendum exists and is ongoing
    /// @return isDeciding Whether referendum is in deciding phase
    /// @return decidingSince Block number when deciding started
    /// @return confirming Block number when confirming ends (0 if not confirming)
    function getDecidingStatus(
        uint32 referendumIndex
    )
        external
        view
        returns (
            bool exists,
            bool isDeciding,
            uint32 decidingSince,
            uint32 confirming
        );

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
}

```

### IGovernance (Combined Interface)

```solidity
/// @title Minimal Governance Precompile Interface
/// @notice Complete interface for smart contract governance participation
interface IGovernance is IReferenda, IConvictionVoting {

}
```

---

## References

- [Parity Polkadot SDK](https://github.com/paritytech/polkadot-sdk/)
- [frame_support docs](https://paritytech.github.io/polkadot-sdk/master/frame_support/index.html)
- [Polkadot Fellows Runtimes](https://github.com/polkadot-fellows/runtimes)
- [Polkassembly Governance UI](https://github.com/polkassembly/governance-ui/tree/main)
- [Subsquare](https://github.com/opensquare-network/subsquare)
