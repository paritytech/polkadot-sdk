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

//! Module with configuration which reflects BridgeHubWococo runtime setup
//! (AccountId, Headers, Hashes...)

#![cfg_attr(not(feature = "std"), no_std)]

pub use bp_bridge_hub_cumulus::*;
use bp_messages::*;
use bp_runtime::{
	decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis, Chain, Parachain,
};
use frame_support::{dispatch::DispatchClass, RuntimeDebug};
use sp_std::prelude::*;

/// BridgeHubWococo parachain.
#[derive(RuntimeDebug)]
pub struct BridgeHubWococo;

impl Chain for BridgeHubWococo {
	type Hash = Hash;
	type Hasher = Hasher;
	type Block = Block;
	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = Nonce;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		*BlockLength::get().max.get(DispatchClass::Normal)
	}

	fn max_extrinsic_weight() -> Weight {
		BlockWeights::get()
			.get(DispatchClass::Normal)
			.max_extrinsic
			.unwrap_or(Weight::MAX)
	}
}

impl Parachain for BridgeHubWococo {
	const PARACHAIN_ID: u32 = BRIDGE_HUB_WOCOCO_PARACHAIN_ID;
}

/// Identifier of BridgeHubWococo in the Wococo relay chain.
pub const BRIDGE_HUB_WOCOCO_PARACHAIN_ID: u32 = 1014;

/// Name of the With-BridgeHubWococo messages pallet instance that is deployed at bridged chains.
pub const WITH_BRIDGE_HUB_WOCOCO_MESSAGES_PALLET_NAME: &str = "BridgeWococoMessages";

/// Name of the With-BridgeHubWococo bridge-relayers pallet instance that is deployed at bridged
/// chains.
pub const WITH_BRIDGE_HUB_WOCOCO_RELAYERS_PALLET_NAME: &str = "BridgeRelayers";

decl_bridge_finality_runtime_apis!(bridge_hub_wococo);
decl_bridge_messages_runtime_apis!(bridge_hub_wococo);
