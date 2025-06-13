// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Defines all functions that can be used to interact with XCM
/// @dev Parameters MUST use SCALE codec serialisation
interface IXcm {
    /// Weight v2
    struct Weight {
        /// The computational time used to execute some logic based on reference hardware
        uint64 refTime;
        /// The size of the proof needed to execute some logic
        uint64 proofSize;
    }

    /// @notice Execute a Versioned XCM message locally with the caller's origin
    /// @param message The Versioned XCM message to send
    /// @param weight The maximum amount of weight to be used to execute the message
    function xcmExecute(bytes calldata message, Weight calldata weight) external;

    /// @notice Send an Versioned XCM message to a destination chain
    /// @param destination The destination location, encoded according to the XCM format
    /// @param message The Versioned XCM message to send
    function xcmSend(bytes calldata destination, bytes calldata message) external;

    /// @notice Given a message estimate the weight cost
    /// @param message The XCM message to send
    /// @returns weight estimated for sending the message
    function weighMessage(bytes calldata message) external view returns (Weight memory weight);
}
