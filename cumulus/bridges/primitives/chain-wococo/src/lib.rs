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
pub use bp_rococo::{
	SS58Prefix, MAX_AUTHORITIES_COUNT, MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE, PARAS_PALLET_NAME,
};

use bp_header_chain::ChainWithGrandpa;
use bp_runtime::{decl_bridge_finality_runtime_apis, Chain};
use frame_support::weights::Weight;

/// Wococo Chain
pub struct Wococo;

impl Chain for Wococo {
	type Block = <PolkadotLike as Chain>::Block;
	type Hash = <PolkadotLike as Chain>::Hash;
	type Hasher = <PolkadotLike as Chain>::Hasher;
	type AccountId = <PolkadotLike as Chain>::AccountId;
	type Balance = <PolkadotLike as Chain>::Balance;
	type Nonce = <PolkadotLike as Chain>::Nonce;
	type Signature = <PolkadotLike as Chain>::Signature;

	fn max_extrinsic_size() -> u32 {
		PolkadotLike::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		PolkadotLike::max_extrinsic_weight()
	}
}

impl ChainWithGrandpa for Wococo {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = WITH_WOCOCO_GRANDPA_PALLET_NAME;
	const MAX_AUTHORITIES_COUNT: u32 = MAX_AUTHORITIES_COUNT;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 =
		REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY;
	const MAX_HEADER_SIZE: u32 = MAX_HEADER_SIZE;
	const AVERAGE_HEADER_SIZE_IN_JUSTIFICATION: u32 = AVERAGE_HEADER_SIZE_IN_JUSTIFICATION;
}

/// Name of the With-Wococo GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_WOCOCO_GRANDPA_PALLET_NAME: &str = "BridgeWococoGrandpa";

decl_bridge_finality_runtime_apis!(wococo);
