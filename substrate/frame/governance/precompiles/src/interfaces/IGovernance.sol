// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "./IReferenda.sol";
import "./IConvictionVoting.sol";

/// @title Minmal Governance Precompile Interface
/// @notice A interface for interacting with on-chain governance.

interface IGovernance is IReferenda, IConvictionVoting {

}
