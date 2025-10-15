// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import "./IReferenda.sol";
import "./IConvictionVoting.sol";

/// @title Governance Precompile Interface
/// @notice A interface for interacting with on-chain governance.
/// It forwards calls directly to the corresponding dispatchable functions,
/// providing access to the `pallet_conviction_voting` and `pallet_referenda` functionalities.
/// @dev See {IReferenda} and {IConvictionVoting} for details.
interface IGovernance is IReferenda, IConvictionVoting {

}
