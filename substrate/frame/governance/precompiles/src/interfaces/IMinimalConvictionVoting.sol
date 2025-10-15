// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

/// @title Minimal ConvictionVoting Interface
/// @dev Provides 8 core functions for voting and delegation
interface IMinimalConvictionVoting {
    enum Conviction {
        None,      // 0.1x votes, unlocked
        Locked1x,  // 1x votes, locked for 1x enactment period
        Locked2x,  // 2x votes, locked for 2x enactment periods
        Locked3x,  // 3x votes, locked for 4x enactment periods
        Locked4x,  // 4x votes, locked for 8x enactment periods
        Locked5x,  // 5x votes, locked for 16x enactment periods
        Locked6x   // 6x votes, locked for 32x enactment periods
    }

    enum VotingType {
        Standard,      // One-way vote with conviction
        Split,         // Split aye/nay, no conviction
        SplitAbstain   // Split aye/nay/abstain, no conviction
    }

    /// @notice Cast a standard vote (aye or nay) with conviction
    function voteStandard(
        uint32 referendumIndex,
        bool aye,
        Conviction conviction,
        uint128 balance
    ) external;

    /// @notice Cast a split vote with explicit aye and nay balances
    function voteSplit(
        uint32 referendumIndex,
        uint128 ayeAmount,
        uint128 nayAmount
    ) external;

    /// @notice Cast a split vote with aye, nay, and abstain balances
    function voteSplitAbstain(
        uint32 referendumIndex,
        uint128 ayeAmount,
        uint128 nayAmount,
        uint128 abstainAmount
    ) external;

    /// @notice Remove a vote from a referendum
    function removeVote(uint16 trackId, uint32 referendumIndex) external;

    /// @notice Delegate voting power to another account
    /// @param to The account to delegate to (32-byte AccountId32)
    function delegate(
        uint16 trackId,
        address to,
        Conviction conviction,
        uint128 balance
    ) external;

    /// @notice Remove delegation within a governance track
    function undelegate(uint16 trackId) external;

    /// @notice Get vote details for a specific referendum
    /// @return exists Whether a vote exists
    /// @return votingType The type of vote cast
    /// @return aye True if standard vote is aye, false for nay
    /// @return ayeAmount Aye balance (pre-conviction)
    /// @return nayAmount Nay balance (pre-conviction)
    /// @return abstainAmount Abstain balance (only for SplitAbstain)
    /// @return conviction Conviction level (only for Standard votes)
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

    /// @notice Get delegation details for an account
    /// @return target The delegated-to account (0x0 if no delegation)
    /// @return balance Amount delegated (pre-conviction)
    /// @return conviction Conviction level applied
    function getDelegation(
        address who,
        uint16 trackId
    )
        external
        view
        returns (address target, uint128 balance, Conviction conviction);
}