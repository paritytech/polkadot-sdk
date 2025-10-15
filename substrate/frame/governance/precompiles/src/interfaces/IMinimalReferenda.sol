// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;
/// @title Minimal Referenda Interface
/// @dev Provides 7 core functions for referendum submission and queries
interface IMinimalReferenda {
    enum Timing {
        AtBlock,     // Enact at specific block number
        AfterBlock   // Enact after N blocks from approval
    }

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

    enum ReferendumStatus {
        Ongoing,
        Approved,
        Rejected,
        Cancelled,
        TimedOut,
        Killed
    }

    enum OngoingPhase {
        AwaitingDeposit,  // Waiting for decision deposit
        Preparing,        // Decision deposit placed, preparing
        Queued,           // Ready but waiting for track space
        Deciding,         // Active voting period
        Confirming        // Passing, in confirmation period
    }

    /// @notice Submit a referendum via preimage lookup (for large proposals)
    /// @dev Requires prior call to pallet_preimage::note_preimage() via extrinsic
    function submitLookup(
        GovernanceOrigin origin,
        bytes32 hash,
        uint32 preimageLength,
        Timing timing,
        uint32 enactmentMoment
    ) external payable returns (uint32 referendumIndex);

    /// @notice Submit a referendum inline (for small proposals)
    function submitInline(
        GovernanceOrigin origin,
        bytes calldata proposal,
        Timing timing,
        uint32 enactmentMoment
    ) external payable returns (uint32 referendumIndex);

    /// @notice Place the decision deposit for a referendum
    function placeDecisionDeposit(uint32 referendumIndex) external payable;

    /// @notice Get comprehensive referendum information
    /// @return exists Whether the referendum exists
    /// @return status Current status (Ongoing, Approved, etc.)
    /// @return ongoingPhase Sub-phase if status is Ongoing
    /// @return trackId The governance track ID
    /// @return proposalHash Hash of the proposal
    /// @return submissionDeposit Submission deposit amount
    /// @return decisionDeposit Decision deposit amount
    /// @return enactmentBlock Block number for execution (if approved)
    /// @return submissionBlock Block when referendum was submitted
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
    /// @return exists Whether referendum exists and is ongoing
    /// @return ayes Aye votes (post-conviction)
    /// @return nays Nay votes (post-conviction)
    /// @return support Aye votes (pre-conviction, for turnout calculation)
    function getReferendumTally(uint32 referendumIndex)
        external
        view
        returns (bool exists, uint128 ayes, uint128 nays, uint128 support);

    /// @notice Check if referendum would pass if ended now
    function isReferendumPassing(uint32 referendumIndex)
        external
        view
        returns (bool exists, bool passing);

    /// @notice Get the submission deposit amount required
    function submissionDeposit() external view returns (uint128);
}