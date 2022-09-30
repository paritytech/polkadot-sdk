// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]
// RuntimeApi generated functions
#![allow(clippy::too_many_arguments)]

pub use bp_polkadot_core::*;
use bp_runtime::decl_bridge_finality_runtime_apis;

/// Polkadot Chain
pub type Polkadot = PolkadotLike;

/// The target length of a session (how often authorities change) on Polkadot measured in of number
/// of blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = 4 * time_units::HOURS;

/// Name of the With-Polkadot GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_POLKADOT_GRANDPA_PALLET_NAME: &str = "BridgePolkadotGrandpa";

decl_bridge_finality_runtime_apis!(polkadot);
