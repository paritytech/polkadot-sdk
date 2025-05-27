// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Defines all functions that can be used to interact with XCM
/// @author Tiago Bandeira
/// @dev Parameters MUST use SCALE codec serialisation
interface IXcm {
    struct Weight {
        uint64 refTime;
        uint64 proofSize;
    }

    /// @notice Execute an XCM message locally with the caller's origin
    /// @param message The XCM message to send
    /// @param weight The maximum amount of weight to be used to execute the message
    function xcmExecute(bytes calldata message, Weight calldata weight) external;

    /// @notice Send an XCM message to a destination chain
    /// @param destination The destination location, encoded according to the XCM format
    /// @param message The XCM message to send
    function xcmSend(bytes calldata destination, bytes calldata message) external;

    /// @notice Given a message estimate the weight cost
    /// @param message The XCM message to send
    /// @returns weight estimated for sending the message
    function weightMessage(bytes calldata message) external view returns (Weight weight);
}