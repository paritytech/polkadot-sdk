// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.22;

import {IERC20} from "./interfaces/IERC20.sol";
import {IGateway} from "./interfaces/IGateway.sol";

import {SafeTokenTransferFrom} from "./utils/SafeTransfer.sol";

import {AssetsStorage, TokenInfo} from "./storage/AssetsStorage.sol";
import {SubstrateTypes} from "./SubstrateTypes.sol";
import {ParaID, MultiAddress, Ticket, Costs} from "./Types.sol";
import {Address} from "./utils/Address.sol";

/// @title Library for implementing Ethereum->Polkadot ERC20 transfers.
library Assets {
    using Address for address;
    using SafeTokenTransferFrom for IERC20;

    /* Errors */
    error InvalidToken();
    error InvalidAmount();
    error InvalidDestination();
    error TokenNotRegistered();
    error Unsupported();

    function isTokenRegistered(address token) external view returns (bool) {
        return AssetsStorage.layout().tokenRegistry[token].isRegistered;
    }

    /// @dev transfer tokens from the sender to the specified agent
    function _transferToAgent(address agent, address token, address sender, uint128 amount) internal {
        if (!token.isContract()) {
            revert InvalidToken();
        }

        if (amount == 0) {
            revert InvalidAmount();
        }

        IERC20(token).safeTransferFrom(sender, agent, amount);
    }

    function sendTokenCosts(address token, ParaID destinationChain, uint128 destinationChainFee)
        external
        view
        returns (Costs memory costs)
    {
        AssetsStorage.Layout storage $ = AssetsStorage.layout();
        TokenInfo storage info = $.tokenRegistry[token];
        if (!info.isRegistered) {
            revert TokenNotRegistered();
        }

        return _sendTokenCosts(destinationChain, destinationChainFee);
    }

    function _sendTokenCosts(ParaID destinationChain, uint128 destinationChainFee)
        internal
        view
        returns (Costs memory costs)
    {
        AssetsStorage.Layout storage $ = AssetsStorage.layout();
        if ($.assetHubParaID == destinationChain) {
            costs.foreign = $.assetHubReserveTransferFee;
        } else {
            // If the final destination chain is not AssetHub, then the fee needs to additionally
            // include the cost of executing an XCM on the final destination parachain.
            costs.foreign = $.assetHubReserveTransferFee + destinationChainFee;
        }
        // We don't charge any extra fees beyond delivery costs
        costs.native = 0;
    }

    function sendToken(
        address token,
        address sender,
        ParaID destinationChain,
        MultiAddress calldata destinationAddress,
        uint128 destinationChainFee,
        uint128 amount
    ) external returns (Ticket memory ticket) {
        AssetsStorage.Layout storage $ = AssetsStorage.layout();

        TokenInfo storage info = $.tokenRegistry[token];
        if (!info.isRegistered) {
            revert TokenNotRegistered();
        }

        // Lock the funds into AssetHub's agent contract
        _transferToAgent($.assetHubAgent, token, sender, amount);

        ticket.dest = $.assetHubParaID;
        ticket.costs = _sendTokenCosts(destinationChain, destinationChainFee);

        // Construct a message payload
        if (destinationChain == $.assetHubParaID) {
            // The funds will be minted into the receiver's account on AssetHub
            if (destinationAddress.isAddress32()) {
                // The receiver has a 32-byte account ID
                ticket.payload = SubstrateTypes.SendTokenToAssetHubAddress32(
                    token, destinationAddress.asAddress32(), $.assetHubReserveTransferFee, amount
                );
            } else {
                // AssetHub does not support 20-byte account IDs
                revert Unsupported();
            }
        } else {
            // The funds will be minted into sovereign account of the destination parachain on AssetHub,
            // and then reserve-transferred to the receiver's account on the destination parachain.
            if (destinationAddress.isAddress32()) {
                // The receiver has a 32-byte account ID
                ticket.payload = SubstrateTypes.SendTokenToAddress32(
                    token,
                    destinationChain,
                    destinationAddress.asAddress32(),
                    $.assetHubReserveTransferFee,
                    destinationChainFee,
                    amount
                );
            } else if (destinationAddress.isAddress20()) {
                // The receiver has a 20-byte account ID
                ticket.payload = SubstrateTypes.SendTokenToAddress20(
                    token,
                    destinationChain,
                    destinationAddress.asAddress20(),
                    $.assetHubReserveTransferFee,
                    destinationChainFee,
                    amount
                );
            } else {
                revert Unsupported();
            }
        }
        emit IGateway.TokenSent(sender, token, destinationChain, destinationAddress, amount);
    }

    function registerTokenCosts() external view returns (Costs memory costs) {
        return _registerTokenCosts();
    }

    function _registerTokenCosts() internal view returns (Costs memory costs) {
        AssetsStorage.Layout storage $ = AssetsStorage.layout();

        // Cost of registering this asset on AssetHub
        costs.foreign = $.assetHubCreateAssetFee;

        // Extra fee to prevent spamming
        costs.native = $.registerTokenFee;
    }

    /// @dev Registers a token (only native tokens at this time)
    /// @param token The ERC20 token address.
    function registerToken(address token) external returns (Ticket memory ticket) {
        if (!token.isContract()) {
            revert InvalidToken();
        }

        AssetsStorage.Layout storage $ = AssetsStorage.layout();

        // NOTE: Explicitly allow a token to be re-registered. This offers resiliency
        // in case a previous registration attempt of the same token failed on the remote side.
        // It means that registration can be retried.
        TokenInfo storage info = $.tokenRegistry[token];
        info.isRegistered = true;

        ticket.dest = $.assetHubParaID;
        ticket.costs = _registerTokenCosts();
        ticket.payload = SubstrateTypes.RegisterToken(token, $.assetHubCreateAssetFee);

        emit IGateway.TokenRegistrationSent(token);
    }
}
