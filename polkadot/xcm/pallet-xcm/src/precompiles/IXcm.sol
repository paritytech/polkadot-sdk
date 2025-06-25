// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title An interface for interacting with `pallet_xcm`
/// @notice Provides functions for executing and sending XCM messages.
/// Learn more about XCM: https://docs.polkadot.com/develop/interoperability/
/// @dev All parameters must be encoded using the SCALE codec.
interface IXcm {
    /// @notice Weight v2
    struct Weight {
        /// The computational time used to execute some logic based on reference hardware.
        uint64 refTime;
        /// The size of the proof needed to execute some logic.
        uint64 proofSize;
    }

    /// @notice Executes an XCM message locally on the current chain with the caller's origin.
    /// @dev Internally calls `pallet_xcm::execute`.
    /// @param message A SCALE-encoded Versioned XCM message.
    /// @param weight The maximum allowed `Weight` for execution.
    /// @return Raw SCALE-encoded `DispatchResultWithPostInfo`. See more: https://paritytech.github.io/polkadot-sdk/master/frame_support/dispatch/type.DispatchResultWithPostInfo
    function execute(bytes calldata message, Weight calldata weight) external returns (bytes memory);

    /// @notice Sends an XCM message to another parachain or consensus system.
    /// @dev Internally calls `pallet_xcm::send`.
    /// @param destination SCALE-encoded destination MultiLocation.
    /// @param message SCALE-encoded Versioned XCM message.
    /// @return Raw SCALE-encoded `DispatchResult`. See more: https://paritytech.github.io/polkadot-sdk/master/frame_support/dispatch/type.DispatchResult
    function send(bytes calldata destination, bytes calldata message) external returns (bytes memory);

    /// @notice Estimates the `Weight` required to execute a given XCM message.
    /// @param message SCALE-encoded Versioned XCM message to analyze.
    /// @return weight Struct containing estimated `refTime` and `proofSize`.
    function weighMessage(bytes calldata message) external view returns (Weight memory weight);
}
