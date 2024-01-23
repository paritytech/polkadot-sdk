// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pragma solidity 0.8.23;

/// @title An agent contract that acts on behalf of a consensus system on Polkadot
/// @dev Instances of this contract act as an agents for arbitrary consensus systems on Polkadot. These consensus systems
/// can include toplevel parachains as as well as nested consensus systems within a parachain.
contract Agent {
    error Unauthorized();

    /// @dev The unique ID for this agent, derived from the MultiLocation of the corresponding consensus system on Polkadot
    bytes32 public immutable AGENT_ID;

    /// @dev The gateway contract controlling this agent
    address public immutable GATEWAY;

    constructor(bytes32 agentID) {
        AGENT_ID = agentID;
        GATEWAY = msg.sender;
    }

    /// @dev Agents can receive ether permissionlessly.
    /// This is important, as agents for top-level parachains also act as sovereign accounts from which message relayers
    /// are rewarded.
    receive() external payable {}

    /// @dev Allow the gateway to invoke some code within the context of this agent
    /// using `delegatecall`. Typically this code will be provided by `AgentExecutor.sol`.
    function invoke(address executor, bytes calldata data) external returns (bool, bytes memory) {
        if (msg.sender != GATEWAY) {
            revert Unauthorized();
        }
        return executor.delegatecall(data);
    }
}
