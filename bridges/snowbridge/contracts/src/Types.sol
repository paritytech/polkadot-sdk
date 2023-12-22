// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.22;

import {
    MultiAddress, multiAddressFromUint32, multiAddressFromBytes32, multiAddressFromBytes20
} from "./MultiAddress.sol";

import {UD60x18} from "prb/math/src/UD60x18.sol";

type ParaID is uint32;

using {ParaIDEq as ==, ParaIDNe as !=, into} for ParaID global;

function ParaIDEq(ParaID a, ParaID b) pure returns (bool) {
    return ParaID.unwrap(a) == ParaID.unwrap(b);
}

function ParaIDNe(ParaID a, ParaID b) pure returns (bool) {
    return !ParaIDEq(a, b);
}

function into(ParaID paraID) pure returns (ChannelID) {
    return ChannelID.wrap(keccak256(abi.encodePacked("para", ParaID.unwrap(paraID))));
}

type ChannelID is bytes32;

using {ChannelIDEq as ==, ChannelIDNe as !=} for ChannelID global;

function ChannelIDEq(ChannelID a, ChannelID b) pure returns (bool) {
    return ChannelID.unwrap(a) == ChannelID.unwrap(b);
}

function ChannelIDNe(ChannelID a, ChannelID b) pure returns (bool) {
    return !ChannelIDEq(a, b);
}

/// @dev A messaging channel for a Polkadot parachain
struct Channel {
    /// @dev The operating mode for this channel. Can be used to
    /// disable messaging on a per-channel basis.
    OperatingMode mode;
    /// @dev The current nonce for the inbound lane
    uint64 inboundNonce;
    /// @dev The current node for the outbound lane
    uint64 outboundNonce;
    /// @dev The address of the agent of the parachain owning this channel
    address agent;
}

/// @dev Inbound message from a Polkadot parachain (via BridgeHub)
struct InboundMessage {
    /// @dev The parachain from which this message originated
    ChannelID channelID;
    /// @dev The channel nonce
    uint64 nonce;
    /// @dev The command to execute
    Command command;
    /// @dev The Parameters for the command
    bytes params;
    /// @dev The maximum gas allowed for message dispatch
    uint64 maxDispatchGas;
    /// @dev The maximum fee per gas
    uint256 maxFeePerGas;
    /// @dev The reward for message submission
    uint256 reward;
    /// @dev ID for this message
    bytes32 id;
}

enum OperatingMode {
    Normal,
    RejectingOutboundMessages
}

/// @dev Messages from Polkadot take the form of these commands.
enum Command {
    AgentExecute,
    Upgrade,
    CreateAgent,
    CreateChannel,
    UpdateChannel,
    SetOperatingMode,
    TransferNativeFromAgent,
    SetTokenTransferFees,
    SetPricingParameters
}

enum AgentExecuteCommand {TransferToken}

/// @dev Application-level costs for a message
struct Costs {
    /// @dev Costs in foreign currency
    uint256 foreign;
    /// @dev Costs in native currency
    uint256 native;
}

struct Ticket {
    ParaID dest;
    Costs costs;
    bytes payload;
}

struct TokenInfo {
    bool isRegistered;
    bytes31 __padding;
}
