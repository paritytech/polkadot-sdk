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
//!
//! but actually this is just reexported BridgeHubRococo stuff, because they are supposed to be
//! identical, at least uses the same parachain runtime

#![cfg_attr(not(feature = "std"), no_std)]

// Re-export only what is really needed
pub use bp_bridge_hub_rococo::{
	account_info_storage_key, AccountId, AccountPublic, AccountSigner, Address, Balance,
	BlockNumber, Hash, Hashing, Header, Nonce, SS58Prefix, Signature, SignedBlock,
	SignedExtensions, UncheckedExtrinsic, WeightToFee, ADDITIONAL_MESSAGE_BYTE_DELIVERY_WEIGHT,
	DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT, EXTRA_STORAGE_PROOF_SIZE,
	MAX_SINGLE_MESSAGE_DELIVERY_CONFIRMATION_TX_WEIGHT,
	MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX, MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
	PAY_INBOUND_DISPATCH_FEE_WEIGHT, TX_EXTRA_BYTES,
};
use bp_messages::*;
use bp_runtime::{decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis};
use frame_support::{sp_runtime::FixedU128, Parameter};
use sp_std::prelude::*;

pub type BridgeHubWococo = bp_bridge_hub_rococo::BridgeHubRococo;

/// Identifier of BridgeHubWococo in the Wococo relay chain.
pub const BRIDGE_HUB_WOCOCO_PARACHAIN_ID: u32 = 1013;

/// Name of the With-BridgeHubWococo messages pallet instance that is deployed at bridged chains.
pub const WITH_BRIDGE_HUB_WOCOCO_MESSAGES_PALLET_NAME: &str = "BridgeWococoMessages";

decl_bridge_finality_runtime_apis!(bridge_hub_wococo);
decl_bridge_messages_runtime_apis!(bridge_hub_wococo);
