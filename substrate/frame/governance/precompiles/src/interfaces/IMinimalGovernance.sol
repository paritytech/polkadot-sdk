// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "./IMinimalReferenda.sol";
import "./IMinimalConvictionVoting.sol";

/// @title Minmal Governance Precompile Interface
/// @notice A interface for interacting with on-chain governance.

interface IMinimalGovernance is IMinimalReferenda, IMinimalConvictionVoting {

}
