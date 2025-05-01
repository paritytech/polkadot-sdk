// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Bridge definitions that can be used by multiple BridgeHub flavors.
//! All configurations here should be dedicated to a single chain; in other words, we don't need two
//! chains for a single pallet configuration.
//!
//! For example, the messaging pallet needs to know the sending and receiving chains, but the
//! GRANDPA tracking pallet only needs to be aware of one chain.

use super::{
	weights, AccountId, Balance, Balances, BlockNumber, PolkadotXcm, Runtime, RuntimeEvent,
};
use alloc::{vec, vec::Vec};
use bp_messages::Weight;
use bp_polkadot_core::parachains::{ParaHead, ParaId};
use bp_relayers::RewardsAccountParams;
use bp_runtime::HeaderOf;
use codec::{Decode, Encode};
use frame_support::{parameter_types, traits::ConstU32};
use rococo_runtime_constants::system_parachain::ASSET_HUB_ID;
use sp_runtime::traits::Header;
use xcm::latest::prelude::*;

parameter_types! {
	pub AssetHubLocation: Location = Location::new(1, [Parachain(ASSET_HUB_ID)]);
	pub const RelayChainHeadersToKeep: u32 = 1024;
	pub const ParachainHeadsToKeep: u32 = 64;

	pub const WestendBridgeParachainPalletName: &'static str = bp_westend::PARAS_PALLET_NAME;
	pub const MaxWestendParaHeadDataSize: u32 = bp_westend::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;

	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;
	pub const RelayerStakeLease: u32 = 8;
	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";

	pub storage DeliveryRewardInBalance: u64 = 1_000_000;
}

/// Add GRANDPA bridge pallet to track Westend relay chain.
pub type BridgeGrandpaWestendInstance = pallet_bridge_grandpa::Instance3;
impl pallet_bridge_grandpa::Config<BridgeGrandpaWestendInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_westend::Westend;
	type MaxFreeHeadersPerBlock = ConstU32<4>;
	type FreeHeadersInterval = ConstU32<5>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Westend BridgeHub parachain
pub type BridgeParachainWestendInstance = pallet_bridge_parachains::Instance3;
impl pallet_bridge_parachains::Config<BridgeParachainWestendInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaWestendInstance;
	type ParasPalletName = WestendBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		(bp_bridge_hub_westend::BridgeHubWestend, bp_asset_hub_westend::AssetHubWestend);
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxWestendParaHeadDataSize;
	type OnNewHead = (
		// Sync AHR headers with state roots.
		pallet_bridge_proof_root_sync::impls::SyncParaHeadersFor<
			Runtime,
			AssetHubWestendStateRootSyncInstance,
			bp_asset_hub_westend::AssetHubWestend,
		>,
	);
}

/// `OnSend` implementation that sends validated AHR headers to AHW.
pub struct ToAssetHubRococoProofRootSender;
impl pallet_bridge_proof_root_sync::OnSend<ParaId, ParaHead> for ToAssetHubRococoProofRootSender {
	fn on_send(roots: &Vec<(ParaId, ParaHead)>) {
		// For smaller messages, we just send minimal data.
		let roots = roots
			.iter()
			.filter_map(|(id, head)| {
				let header: HeaderOf<bp_asset_hub_westend::AssetHubWestend> =
					match Decode::decode(&mut &head.0[..]) {
						Ok(header) => header,
						Err(error) => {
							log::warn!(
								target: "runtime::bridge-xcm::on-send",
								"Failed to decode parachain header - skipping it! head: {:?}, para_id: {:?}, error: {:?}",
								head,
								id,
								error,
							);
							return None;
						},
					};
				// We just need block_hash and state_root.
				Some((header.hash(), *header.state_root()))
			})
			.collect::<Vec<_>>();

		// Send dedicated `Transact` to AHW.
		let xcm = Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			Transact {
				origin_kind: OriginKind::Xcm,
				fallback_max_weight: None,
				call: bp_asset_hub_rococo::Call::AssetHubWestendProofRootStore(
					bp_asset_hub_rococo::ProofRootStoreCall::note_new_roots {
						roots: roots.clone(),
					},
				)
				.encode()
				.into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		]);
		if let Err(error) = PolkadotXcm::send_xcm(Here, AssetHubLocation::get(), xcm) {
			log::warn!(target: "runtime::bridge-xcm::on-send", "Failed to send XCM: {:?}", error);
		}
	}

	fn on_send_weight() -> Weight {
		<<Runtime as pallet_xcm::Config>::WeightInfo as pallet_xcm::WeightInfo>::send()
	}
}

/// Simple mechanism that syncs/sends validated Asset Hub Rococo headers to other local chains.
/// For example,
///  1. We need AHR headers for direct bridge messaging on AHW (ToAssetHubRococoProofRootSender).
///  2. We may need AHR headers for D-Day detection on Collectives (ToCollectivesProofRootSender).
pub type AssetHubWestendStateRootSyncInstance = pallet_bridge_proof_root_sync::Instance1;
impl pallet_bridge_proof_root_sync::Config<AssetHubWestendStateRootSyncInstance> for Runtime {
	type Key = ParaId;
	type Value = ParaHead;
	type RootsToKeep = ParachainHeadsToKeep;
	type MaxRootsToSend = ParachainHeadsToKeep;
	type OnSend = (ToAssetHubRococoProofRootSender,);
}

/// Allows collect and claim rewards for relayers
pub type RelayersForLegacyLaneIdsMessagesInstance = ();
impl pallet_bridge_relayers::Config<RelayersForLegacyLaneIdsMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RewardBalance = Balance;
	type Reward = RewardsAccountParams<bp_messages::LegacyLaneId>;
	type PaymentProcedure = bp_relayers::PayRewardFromAccount<
		pallet_balances::Pallet<Runtime>,
		AccountId,
		bp_messages::LegacyLaneId,
		Self::RewardBalance,
	>;
	type StakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
		AccountId,
		BlockNumber,
		Balances,
		RelayerStakeReserveId,
		RequiredStakeForStakeAndSlash,
		RelayerStakeLease,
	>;
	type Balance = Balance;
	type WeightInfo = weights::pallet_bridge_relayers_legacy::WeightInfo<Runtime>;
}

/// Allows collect and claim rewards for relayers
pub type RelayersForPermissionlessLanesInstance = pallet_bridge_relayers::Instance2;
impl pallet_bridge_relayers::Config<RelayersForPermissionlessLanesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RewardBalance = Balance;
	type Reward = RewardsAccountParams<bp_messages::HashedLaneId>;
	type PaymentProcedure = bp_relayers::PayRewardFromAccount<
		pallet_balances::Pallet<Runtime>,
		AccountId,
		bp_messages::HashedLaneId,
		Self::RewardBalance,
	>;
	type StakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
		AccountId,
		BlockNumber,
		Balances,
		RelayerStakeReserveId,
		RequiredStakeForStakeAndSlash,
		RelayerStakeLease,
	>;
	type Balance = Balance;
	type WeightInfo = weights::pallet_bridge_relayers_permissionless_lanes::WeightInfo<Runtime>;
}

/// Add GRANDPA bridge pallet to track Rococo Bulletin chain.
pub type BridgeGrandpaRococoBulletinInstance = pallet_bridge_grandpa::Instance4;
impl pallet_bridge_grandpa::Config<BridgeGrandpaRococoBulletinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_polkadot_bulletin::PolkadotBulletin;
	type MaxFreeHeadersPerBlock = ConstU32<4>;
	type FreeHeadersInterval = ConstU32<5>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	// Technically this is incorrect - we have two pallet instances and ideally we shall
	// benchmark every instance separately. But the benchmarking engine has a flaw - it
	// messes with components. E.g. in Kusama maximal validators count is 1024 and in
	// Bulletin chain it is 100. But benchmarking engine runs Bulletin benchmarks using
	// components range, computed for Kusama => it causes an error.
	//
	// In practice, however, GRANDPA pallet works the same way for all bridged chains, so
	// weights are also the same for both bridges.
	type WeightInfo = weights::pallet_bridge_grandpa::WeightInfo<Runtime>;
}
