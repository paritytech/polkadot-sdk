# Governance Precompile — [#8366](https://github.com/paritytech/polkadot-sdk/issues/8366)

**Authors:** Eman Herawy & Lucas Grasso
**Mentor:** Ankan Anurag — *PBA Bounty Hunters*

##  Prelude

This issue is part of the [OG Rust bounties](https://ogrust.com/), aiming to expand Polkadot’s on-chain functionality to EVM developers by bridging OpenGov to smart contracts.

---

##  Summary
This proposal requests funding to implement an EVM precompile that exposes Polkadot’s OpenGov functionality directly to smart contracts. This precompile will allow EVM developers to submit referenda, vote, delegate, and manage preimages fully on-chain, without relying on Substrate RPCs.
By enabling direct OpenGov interactions from Solidity, we unlock hybrid EVM–Substrate governance dApps, reducing off-chain dependencies, improving UX, and expanding Polkadot’s developer base.
Development will proceed across **xxx milestones**, delivering a Solidity interface, Rust implementation, tests, benchmarks, and documentation. 
The total requested bounty is `??????? DOT`, including implementation, testing, and curation fees. Child bounties may follow for audits or extensions.

---

##  Motivation

In view of the upcoming AH migration, creating a governance precompile will enable smart contracts (thus smart contract users) to directly participate in on-chain decision making.
By exposing key governance functions through a contract friendly interface, DAOs and other contract applications can integrate natively with Polkadot’s governance system each with their own internal voting mechanism.


---

##  Proposed Solidity Interface

We propose a minimal yet complete set of Solidity interfaces to interact with the governance pallets:
```solidity

// SPDX-License-Identifier: GPL-3.0

pragma solidity ^0.8.30;

/// @title Preimage Precompile Interface
interface IPreimage {
    /// @notice Information about a preimage.
    struct Preimage {
        bytes32 hash;
        uint32 len; // @note , luck think this should be uint32, why?
    }

    /// @notice Register a preimage on-chain.
    /// @dev If the preimage was previously requested, no fees or deposits are taken for providing the preimage.
    ///      Otherwise, a deposit is taken proportional to the size of the preimage.
    /// @param data The preimage data to register.
    /// @return preimage The registered preimage information.
    function notePreimage(
        bytes calldata data
    ) external payable returns (Preimage memory preimage);

    /// @notice Clear an unrequested preimage from the runtime storage and refund deposit.
    /// @param hash The hash of the preimage to clear.
    function unnotePreimage(bytes32 hash) external;

    /// @notice Get the preimage data for a given hash.
    /// @param hash The hash of the preimage to query.
    /// @return exists Whether the preimage exists.
    /// @return data The preimage data, if it exists.
    function preImageOf(
        bytes32 hash
    ) external view returns (bool exists, bytes memory data);
}

/// @title ConvictionVoting Interface
interface IConvictionVoting {
    /// @notice A value denoting the strength of conviction of a vote.
    enum Conviction {
        /// 0.1x votes, unlocked.
        None,
        /// 1x votes, locked for an enactment period following a successful vote.
        Locked1x,
        /// 2x votes, locked for 2x enactment periods following a successful vote.
        Locked2x,
        /// 3x votes, locked for 4x...
        Locked3x,
        /// 4x votes, locked for 8x...
        Locked4x,
        /// 5x votes, locked for 16x...
        Locked5x,
        /// 6x votes, locked for 32x...
        Locked6x
    }

    /// @notice The type of vote cast.
    enum VotingType {
        /// No vote. Used to indicate absence of a vote.
        None, 
        /// A standard vote, one-way (approve or reject) with a given amount of conviction.
        Standard,
        /// A split vote with balances given for both ways, and with no conviction.
        Split,
        /// A split vote with balances given for both ways as well as abstentions, and with no conviction.
        SplitAbstain
    }

    /// @notice Cast a standard vote (aye or nay) with conviction.
    /// @param referendumIndex The index of the referendum to vote on.
    /// @param aye True for approving, false for rejecting.
    /// @param conviction Conviction level as defined in the `Conviction` enum.
    /// @param balance The amount of tokens to vote with.
    function castStandardVote(
        uint32 referendumIndex,
        bool aye,
        Conviction conviction,
        uint128 balance
    ) external;

    /// @notice Cast a split vote with explicit aye and nay balances, no conviction/lock applied.
    /// @param referendumIndex The index of the referendum to vote on.
    /// @param ayeAmount Balance allocated to aye.
    /// @param nayAmount Balance allocated to nay.
    function castSplitVote(
        uint32 referendumIndex,
        uint128 ayeAmount,
        uint128 nayAmount
    ) external;

    /// @notice Cast a split vote with explicit aye, nay and abstain balances, no conviction/lock applied.
    /// @param referendumIndex The index of the referendum to vote on.
    /// @param ayeAmount Balance allocated to aye.
    /// @param nayAmount Balance allocated to nay.
    /// @param abstainAmount Balance allocated to abstain.
    function castSplitAbstainVote(
        uint32 referendumIndex,
        uint128 ayeAmount,
        uint128 nayAmount,
        uint128 abstainAmount
    ) external;

    /// @notice Remove a vote from a referendum.
    /// @param trackId The governance track identifier.
    /// @param referendumIndex The referendum index.
    function removeVote(uint16 trackId, uint32 referendumIndex) external;

    /// @notice Unlock expired voting/delegation lock
    /// @param trackId The trackId/track ID to unlock
    /// @param target The account to unlock (can be yourself or others)
    function unlock(uint16 trackId, bytes32 target) external;

    /// @notice Delegate voting power to another account within a specific governance track.
    /// @dev Applies the sender’s balance with the specified conviction multiplier.
    /// @param trackId The governance track identifier.
    /// @param to The account to which voting power is delegated (32-byte bytes32). See https://docs.polkadot.com/polkadot-protocol/smart-contract-basics/accounts/#polkadot-to-ethereum-mapping.
    /// @param conviction Conviction level as defined in the `Conviction` enum.
    function delegate(
        uint16 trackId,
        bytes32 to, // why not address ? when using it in evm , it check it by default , need to investigate further 
        Conviction conviction,
        uint128 balance
    ) external;

    /// @notice Remove any existing delegation within a governance track.
    /// @param trackId The governance track identifier.
    function undelegate(uint16 trackId) external;

    /// @notice Get the locked balance for an account in a trackId
    /// @param who The account to query
    /// @param trackId The governance track to query
    /// @return The locked amount
    function getLockedBalance(
        bytes32 who,
        uint16 trackId
    ) external view returns (uint128);

    /// @notice Get the maximum locked balance across all trackIdes
    /// @param who The account to query
    /// @return The total locked amount (max of all locks along governance tracks)
    function getTotalLocked(bytes32 who) external view returns (uint128);

    /// @notice Get the current delegation details for an account in a governance track
    /// @dev Returns (0x00, 0, Conviction.Null) if there is no delegation.
    /// @param who The account to query
    /// @param trackId The governance track to query
    /// @return delegate The account to which voting power is delegated (32-byte bytes32). Is 0 when there is no delegation. See https://docs.polkadot.com/polkadot-protocol/smart-contract-basics/accounts/#polkadot-to-ethereum-mapping.
    /// @return balance The amount of tokens delegated (pre-conviction).
    /// @return conviction The conviction level applied to the delegation as defined in the `Conviction` enum.
    function getDelegation(
        bytes32 who,
        uint16 trackId
    )
        external
        view
        returns (bytes32 delegate, uint128 balance, Conviction conviction);

    /// @notice Get the current vote details for specific referendum of an account in a governance track
    /// @dev Returns empty array if no votes or if the user is delegating.
    /// @param who The account to query
    /// @param trackId The governance track to query
    /// @param referendumIndex The referendum index to query
    /// @return votingType The type of vote as defined in the `VotingType` enum.
    /// @return ayeAmount The amount of tokens voting aye (pre-conviction). 0 for standard nay votes.
    /// @return nayAmount The amount of tokens voting nay (pre-conviction). 0 for standard aye votes.
    /// @return abstainAmount The amount of tokens voting abstain (pre-conviction). 0 for standard and split votes.
    /// @return conviction The conviction level applied to the vote as defined in the `Conviction` enum. Conviction.Null for split and split-abstain votes.
    function getVoting(
        bytes32 who,
        uint16 trackId,
        uint32 referendumIndex
    )
        external
        view
        returns (
            VotingType votingType,
            uint128 ayeAmount, // in case of standard vote, if aye is false, this will be 0
            uint128 nayAmount, // in case of standard vote, if aye is true, this will be 0
            uint128 abstainAmount,
            Conviction conviction
        );
}
/// @title Referenda Metadata Interface
interface IReferendaMetadata {
    /// @notice Set metadata for a referendum. Only callable by the referendum submitter.
    /// @param referendumIndex The index of the referendum for which to set metadata.
    /// @param metadataHash The hash of the metadata to associate with the referendum.
    function setMetadata(uint32 referendumIndex, bytes32 metadataHash) external;

    /// @notice Clear metadata for a referendum and refund the metadata deposit.
    /// @param referendumIndex The index of the referendum for which to clear metadata.
    function clearMetadata(uint32 referendumIndex) external;

    /// @notice Get metadata hash for a referendum.
    /// @param referendumIndex The index of the referendum to query.
    function getMetadata(
        uint32 referendumIndex
    ) external view returns (bytes32);
}
/// @title Referenda Interface
interface IReferenda is IReferendaMetadata {
    /// @notice Enum representing the `PalletsOrigin` type in Substrate. // 
    // what if the propsoal origin is not in this list? 
    enum PalletsOrigin {
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
    /// @notice When the referendum should be enacted.
    enum DispatchMoment {
        AtBlock,
        AfterBlock
    }
//@note this might be misguided as status is a bit dif ?
    /// @notice Information about a referendum status.
    enum ReferendumStatus {
        /// Referendum has been submitted and is being voted on.
        Ongoing,
        /// Referendum finished with approval. Submission deposit is held.
        Approved,
        /// Referendum finished with rejection. Submission deposit is held.
        Rejected,
        /// Referendum finished with cancellation. Submission deposit is held.
        Cancelled,
        /// Referendum finished and was never decided. Submission deposit is held.
        TimedOut,
        /// Referendum finished with a kill.
        Killed
    }

    
    /// @notice Sub-states for Ongoing referenda
    enum OngoingPhase {
        AwaitingDeposit,  // 0 - Waiting for decision deposit
        Preparing,        // 1 - Decision deposit placed, preparing
        Queued,           // 2 - Ready but waiting for track space
        Deciding,         // 3 - Active voting, not passing yet
        Confirming        // 4 - Passing, in confirmation period
    }
    /// @notice Submit a referendum via preimage lookup (for large proposals). Payable for submission deposit.
    /// @param origin The origin of the proposal.
    /// @param preimage The preimage information (hash and length) of the proposal to be submitted.
    /// @param dispatchMoment When the referendum should be enacted as defined in the `Timing` enum.
    /// @param enactmentMoment The block number for enactment (handles DispatchTime in Substrate).
    /// @return referendumIndex The index of the newly created referendum.
    function submitLookup(
        PalletsOrigin origin, // @note here we have to think , should we use enum or serialize it 
        IPreimage.Preimage calldata preimage,
        DispatchMoment dispatchMoment,
        uint32 enactmentMoment
    ) external payable returns (uint32 referendumIndex);

    /// @notice Submit a referendum inline (for small proposals). Payable for submission deposit.
    /// @param origin The origin of the proposal.
    /// @param proposal The proposal call data to be submitted inline.
    /// @param dispatchMoment When the referendum should be enacted as defined in the `Timing` enum.
    /// @param enactmentMoment The block number for enactment (handles DispatchTime in Substrate).
    /// @return referendumIndex The index of the newly created referendum.
    function submitInline(
        PalletsOrigin origin,
        bytes calldata proposal,
        DispatchMoment dispatchMoment,
        uint32 enactmentMoment
    ) external payable returns (uint32 referendumIndex);

    /// @notice Place the decision deposit for a referendum. Payable (use msg.value matching track's decision_deposit).
    /// @param referendumIndex The index of the referendum for which to place the deposit.
    function placeDecisionDeposit(uint32 referendumIndex) external payable;

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

    /// @notice Check if a referendum would pass if ended now
    /// @param referendumIndex The referendum index
    /// @return exists Whether the referendum exists and is ongoing @note why ?? if it;s not there , it will eb false !!
    /// @return passing Whether the referendum would pass if ended now
    function isReferendumPassing(
        uint32 referendumIndex
    ) external view returns (bool exists, bool passing);

 
    /// @notice Get comprehensive referendum information
    /// @dev This is the primary function - returns all essential data
    /// @param referendumIndex The referendum index
    /// @return exists Whether the referendum exists
    /// @return status Main status (None, Ongoing, Approved, etc.)
    /// @return ongoingPhase If status=Ongoing, which phase (AwaitingDeposit, Deciding, etc.)
    /// @return trackId The governance track/class ID
    /// @return proposalHash Hash of the proposal call
    /// @return submissionDeposit Submission deposit amount (0 if refunded/none)
    /// @return decisionDeposit Decision deposit amount (0 if not placed/refunded)
    /// @return enactmentBlock When approved proposal executes (0 if not approved)
    /// @return submissionBlock When referendum was submitted (0 if doesn't exist)
    function getReferendumInfo(uint32 referendumIndex)
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
    /// @param referendumIndex The referendum index
    /// @return exists Whether the referendum exists and is ongoing
    /// @return ayes The number of aye votes, expressed in terms of post-conviction lock-vote.
    /// @return nays The number of nay votes, expressed in terms of post-conviction lock-vote.
    /// @return support The basic number of aye votes, expressed pre-conviction.
    function getReferendumTally(
        uint32 referendumIndex
    )
        external
        view
        returns (bool exists, uint128 ayes, uint128 nays, uint128 support);

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

    /// @notice Get the submission deposit amount required for submitting a referendum
    /// @return The submission deposit amount
    function submissionDeposit() external view returns (uint128);
}

/// @title Governance Precompile Interface
interface IGovernance is IReferenda, IConvictionVoting {

}

```

## 🌀 Lifecycle Overview

Below is a high-level overview of the interaction flow between an EVM contract and OpenGov pallets through the precompile:

```
+-------------------+
|   EVM Smart       |   (Contract calls precompile functions)
|   Contract        |
|                    |
|   - submitLookup()       | → Submits referendum via lookup
|   - submitInline()       | → Submits referendum inline
|   - voteStandard() | → Casts a standard vote
+-------------------+
          |
          | Calls Precompile
          v
+-------------------+
|   EVM Precompile  |   (Rust implementation in polkadot-sdk)
|   (GovPreCompile) |
|  - submit_lookup()| → Dispatches to pallet_referenda
|  - submit_inline()| → Dispatches to pallet_referenda
|  - vote()         | → Maps vote data, dispatches to pallet_conviction_voting
+-------------------+
          |
          | Dispatches to Pallets
          v
+-------------------+    +-------------------+    +-------------------+
|   pallet_preimage |    | pallet_referenda  |    | pallet_conviction |
|   - note_preimage |    |   - submit        |    |   - vote          |
|   - unnote_preimage|   |   - place_deposit |    |   - delegate      |
|   (Stores preimage)|    |   - refund_deposit|    |   - unlock        |
|                    |    |   - tally         |    |   (Manages votes) |
|                    |    |   (Manages lifecycle) |
+-------------------+    +-------------------+    +-------------------+

```

* EVM contracts call Solidity functions.
* The precompile translates inputs to Substrate types.
* Pallets handle preimage storage, referenda lifecycle, voting, and delegation logic.

---

##  Development Milestones

The implementation is divided into **xxx  milestones**, each unlocking a functional layer of OpenGov interactions. This staged approach ensures stability, testing, and incremental delivery.

---


## References

* [Parity Polkadot SDK](https://github.com/paritytech/polkadot-sdk/)
* [frame_support docs](https://paritytech.github.io/polkadot-sdk/master/frame_support/index.html)
* [Polkadot Fellows Runtimes](https://github.com/polkadot-fellows/runtimes)
* [Polkassembly Governance UI](https://github.com/polkassembly/governance-ui/tree/main)
* [Subsquare](https://github.com/opensquare-network/subsquare)

