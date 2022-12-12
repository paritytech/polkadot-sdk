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
use bp_runtime::{decl_bridge_finality_runtime_apis, Chain, Parachain};
use frame_support::weights::Weight;

/// Westend Chain
pub type Westend = PolkadotLike;

/// Westmint parachain definition
#[derive(Debug, Clone, Copy)]
pub struct Westmint;

// Westmint seems to use the same configuration as all Polkadot-like chains, so we'll use Westend
// primitives here.
impl Chain for Westmint {
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	type Balance = Balance;
	type Index = Nonce;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		Westend::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		Westend::max_extrinsic_weight()
	}
}

impl Parachain for Westmint {
	const PARACHAIN_ID: u32 = WESTMINT_PARACHAIN_ID;
}

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

/// Maximal SCALE-encoded size of parachains headers that are stored at Westend `Paras` pallet.
///
/// It includes the block number and state root, so it shall be near 40 bytes, but let's have some
/// reserve.
pub const MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE: u32 = 128;

/// Identifier of Westmint parachain at the Westend relay chain.
pub const WESTMINT_PARACHAIN_ID: u32 = 2000;

decl_bridge_finality_runtime_apis!(westend);

decl_bridge_finality_runtime_apis!(westmint);
