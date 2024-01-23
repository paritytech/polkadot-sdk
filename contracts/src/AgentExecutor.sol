// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.23;

import {AgentExecuteCommand, ParaID} from "./Types.sol";
import {SubstrateTypes} from "./SubstrateTypes.sol";

import {IERC20} from "./interfaces/IERC20.sol";
import {SafeTokenTransfer, SafeNativeTransfer} from "./utils/SafeTransfer.sol";

/// @title Code which will run within an `Agent` using `delegatecall`.
/// @dev This is a singleton contract, meaning that all agents will execute the same code.
contract AgentExecutor {
    using SafeTokenTransfer for IERC20;
    using SafeNativeTransfer for address payable;

    /// @dev Execute a message which originated from the Polkadot side of the bridge. In other terms,
    /// the `data` parameter is constructed by the BridgeHub parachain.
    ///
    function execute(bytes memory data) external {
        (AgentExecuteCommand command, bytes memory params) = abi.decode(data, (AgentExecuteCommand, bytes));
        if (command == AgentExecuteCommand.TransferToken) {
            (address token, address recipient, uint128 amount) = abi.decode(params, (address, address, uint128));
            _transferToken(token, recipient, amount);
        }
    }

    /// @dev Transfer ether to `recipient`. Unlike `_transferToken` This logic is not nested within `execute`,
    /// as the gateway needs to control an agent's ether balance directly.
    ///
    function transferNative(address payable recipient, uint256 amount) external {
        recipient.safeNativeTransfer(amount);
    }

    /// @dev Transfer ERC20 to `recipient`. Only callable via `execute`.
    function _transferToken(address token, address recipient, uint128 amount) internal {
        IERC20(token).safeTransfer(recipient, amount);
    }
}
