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

/// Westend Chain
pub type Westend = PolkadotLike;

/// Name of the parachains pallet at the Westend runtime.
pub const PARAS_PALLET_NAME: &str = "Paras";

/// Name of the With-Westend GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_WESTEND_GRANDPA_PALLET_NAME: &str = "BridgeWestendGrandpa";
/// Name of the With-Westend parachains bridge pallet instance that is deployed at bridged chains.
pub const WITH_WESTEND_BRIDGE_PARAS_PALLET_NAME: &str = "BridgeWestendParachains";

/// Maximal number of GRANDPA authorities at Westend.
///
/// Corresponds to the `MaxAuthorities` constant value from the Westend runtime configuration.
pub const MAX_AUTHORITIES_COUNT: u32 = 100_000;

/// Maximal SCALE-encoded header size (in bytes) at Westend.
///
/// Let's assume that the largest header is header that enacts new authorities set with
/// `MAX_AUTHORITES_COUNT`. Every authority means 32-byte key and 8-byte weight. Let's also have
/// some fixed reserve for other things (digest, block hash and number, ...) as well.
pub const MAX_HEADER_SIZE: u32 = 4096 + MAX_AUTHORITIES_COUNT * 40;

/// Maximal SCALE-encoded size of parachains headers that are stored at Westend `Paras` pallet.
pub const MAX_NESTED_PARACHAIN_HEAD_SIZE: u32 = MAX_HEADER_SIZE;

/// Identifier of Westmint parachain at the Westend relay chain.
pub const WESTMINT_PARACHAIN_ID: u32 = 2000;

decl_bridge_finality_runtime_apis!(westend);

decl_bridge_finality_runtime_apis!(westmint);
