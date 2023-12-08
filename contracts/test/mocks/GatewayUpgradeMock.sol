// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.22;

import {Channel, InboundMessage, OperatingMode, ParaID, Command, ChannelID, MultiAddress} from "../../src/Types.sol";
import {IGateway} from "../../src/interfaces/IGateway.sol";
import {IInitializable} from "../../src/interfaces/IInitializable.sol";
import {Verification} from "../../src/Verification.sol";
import {UD60x18, convert} from "prb/math/src/UD60x18.sol";

contract GatewayUpgradeMock is IGateway, IInitializable {
    /**
     * Getters
     */

    function operatingMode() external pure returns (OperatingMode) {
        return OperatingMode.Normal;
    }

    function channelOperatingModeOf(ChannelID) external pure returns (OperatingMode) {
        return OperatingMode.Normal;
    }

    function channelNoncesOf(ChannelID) external pure returns (uint64, uint64) {
        return (0, 0);
    }

    function agentOf(bytes32) external pure returns (address) {
        return address(0);
    }

    function implementation() external pure returns (address) {
        return address(0);
    }

    function isTokenRegistered(address) external pure returns (bool) {
        return true;
    }

    function submitV1(InboundMessage calldata, bytes32[] calldata, Verification.Proof calldata) external {}

    function quoteRegisterTokenFee() external pure returns (uint256) {
        return 1;
    }

    function registerToken(address) external payable {}

    function quoteSendTokenFee(address, ParaID, uint128) external pure returns (uint256) {
        return 1;
    }

    function sendToken(address, ParaID, MultiAddress calldata, uint128, uint128) external payable {}

    event Initialized(uint256 d0, uint256 d1);

    function initialize(bytes memory data) external {
        // Just decode and exit
        (uint256 d0, uint256 d1) = abi.decode(data, (uint256, uint256));
        emit Initialized(d0, d1);
    }

    function pricingParameters() external pure returns (UD60x18, uint128) {
        return (convert(0), uint128(0));
    }
}
