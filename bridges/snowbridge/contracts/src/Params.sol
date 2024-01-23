// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.23;

import {ChannelID, OperatingMode} from "./Types.sol";
import {UD60x18} from "prb/math/src/UD60x18.sol";

// Payload for AgentExecute
struct AgentExecuteParams {
    bytes32 agentID;
    bytes payload;
}

// Payload for CreateAgent
struct CreateAgentParams {
    /// @dev The agent ID of the consensus system
    bytes32 agentID;
}

// Payload for CreateChannel
struct CreateChannelParams {
    /// @dev The channel ID
    ChannelID channelID;
    /// @dev The agent ID
    bytes32 agentID;
    /// @dev Initial operating mode
    OperatingMode mode;
}

// Payload for UpdateChannel
struct UpdateChannelParams {
    /// @dev The parachain used to identify the channel to update
    ChannelID channelID;
    /// @dev The new operating mode
    OperatingMode mode;
}

// Payload for Upgrade
struct UpgradeParams {
    /// @dev The address of the implementation contract
    address impl;
    /// @dev the codehash of the new implementation contract.
    /// Used to ensure the implementation isn't updated while
    /// the upgrade is in flight
    bytes32 implCodeHash;
    /// @dev parameters used to upgrade storage of the gateway
    bytes initParams;
}

// Payload for SetOperatingMode
struct SetOperatingModeParams {
    /// @dev The new operating mode
    OperatingMode mode;
}

// Payload for TransferNativeFromAgent
struct TransferNativeFromAgentParams {
    /// @dev The ID of the agent to transfer funds from
    bytes32 agentID;
    /// @dev The recipient of the funds
    address recipient;
    /// @dev The amount to transfer
    uint256 amount;
}

// Payload for SetTokenTransferFees
struct SetTokenTransferFeesParams {
    /// @dev The remote fee (DOT) for registering a token on AssetHub
    uint128 assetHubCreateAssetFee;
    /// @dev The remote fee (DOT) for send tokens to AssetHub
    uint128 assetHubReserveTransferFee;
    /// @dev extra fee to register an asset and discourage spamming (Ether)
    uint256 registerTokenFee;
}

// Payload for SetPricingParameters
struct SetPricingParametersParams {
    /// @dev The ETH/DOT exchange rate
    UD60x18 exchangeRate;
    /// @dev The cost of delivering messages to BridgeHub in DOT
    uint128 deliveryCost;
}
