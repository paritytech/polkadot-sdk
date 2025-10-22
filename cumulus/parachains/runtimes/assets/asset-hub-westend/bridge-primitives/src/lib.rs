// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Module with configuration which reflects AssetHubWestend runtime setup.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use bp_bridge_hub_cumulus::*;
use bp_messages::*;
use bp_runtime::{
	decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis, Chain, ChainId, Parachain,
};
pub use bp_xcm_bridge_hub_router::XcmBridgeHubRouterCall;
use codec::{Decode, Encode};
use frame_support::{
	dispatch::DispatchClass,
	sp_runtime::{MultiAddress, MultiSigner, RuntimeDebug, StateVersion},
};
use scale_info::TypeInfo;
use testnet_parachains_constants::westend::currency::UNITS;
use xcm::latest::prelude::*;

/// `AssetHubWestend` Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to `AssetHubWestend` chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with
/// `AssetHubWestend` `construct_runtime`, so that we maintain SCALE-compatibility.
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// `ToRococoXcmRouter` bridge pallet.
	#[codec(index = 34)]
	ToRococoXcmRouter(XcmBridgeHubRouterCall),
}

frame_support::parameter_types! {
	/// Some sane weight to execute `xcm::Transact(pallet-xcm-bridge-hub-router::Call::report_bridge_status)`.
	pub const XcmBridgeHubRouterTransactCallMaxWeight: frame_support::weights::Weight = frame_support::weights::Weight::from_parts(200_000_000, 6144);

	/// Should match the `AssetDeposit` of the `ForeignAssets` pallet on Asset Hub.
	pub const CreateForeignAssetDeposit: u128 = UNITS / 10;
}

/// Builds an (un)congestion XCM program with the `report_bridge_status` call for
/// `ToRococoXcmRouter`.
pub fn build_congestion_message<RuntimeCall>(
	bridge_id: sp_core::H256,
	is_congested: bool,
) -> alloc::vec::Vec<Instruction<RuntimeCall>> {
	alloc::vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact {
			origin_kind: OriginKind::Xcm,
			fallback_max_weight: Some(XcmBridgeHubRouterTransactCallMaxWeight::get()),
			call: Call::ToRococoXcmRouter(XcmBridgeHubRouterCall::report_bridge_status {
				bridge_id,
				is_congested,
			})
			.encode()
			.into(),
		},
		ExpectTransactStatus(MaybeErrorCode::Success),
	]
}

/// Identifier of AssetHubWestend in the Westend relay chain.
pub const ASSET_HUB_WESTEND_PARACHAIN_ID: u32 = 1000;

/// AssetHubWestend parachain.
#[derive(RuntimeDebug)]
pub struct AssetHubWestend;

impl Chain for AssetHubWestend {
	const ID: ChainId = *b"ahwd";

	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = Nonce;
	type Signature = Signature;

	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		*BlockLength::get().max.get(DispatchClass::Normal)
	}

	fn max_extrinsic_weight() -> Weight {
		BlockWeightsForAsyncBacking::get()
			.get(DispatchClass::Normal)
			.max_extrinsic
			.unwrap_or(Weight::MAX)
	}
}

impl Parachain for AssetHubWestend {
	const PARACHAIN_ID: u32 = ASSET_HUB_WESTEND_PARACHAIN_ID;
	const MAX_HEADER_SIZE: u32 = MAX_ASSET_HUB_HEADER_SIZE;
}

/// Describing permissionless lanes instance
impl ChainWithMessages for AssetHubWestend {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		WITH_ASSET_HUB_WESTEND_MESSAGES_PALLET_NAME;

	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
}

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// The address format for describing accounts.
pub type Address = MultiAddress<AccountId, ()>;

/// Name of the With-AssetHubWestend messages pallet instance that is deployed at bridged chains.
pub const WITH_ASSET_HUB_WESTEND_MESSAGES_PALLET_NAME: &str = "BridgeWestendMessages";

/// Name of the With-AssetHubWestend bridge-relayers pallet instance that is deployed at bridged
/// chains.
pub const WITH_ASSET_HUB_WESTEND_RELAYERS_PALLET_NAME: &str = "BridgeRelayers";

/// Pallet index of `BridgeRococoMessages: pallet_bridge_messages::<Instance1>`.
pub const WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX: u8 = 63;

decl_bridge_finality_runtime_apis!(asset_hub_westend);
decl_bridge_messages_runtime_apis!(asset_hub_westend, HashedLaneId);
