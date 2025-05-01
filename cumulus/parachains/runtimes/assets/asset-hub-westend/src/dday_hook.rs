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

//! This hook pallet is used to send data to the Collectives.
//! It sends:
//! - The parent block
//!
//! Note: This hook and the corresponding XCM would not be necessary if:
//! - A mechanism is implemented to read the custom `RelayChainStateProof` context, allowing us to
//!   retrieve the latest AssetHub head directly on Collectives.
//!   - See: https://github.com/paritytech/polkadot-sdk/issues/7445
//!   - See: https://github.com/paritytech/polkadot-sdk/issues/82

use crate::{DDayHook, PolkadotXcm, Runtime};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use cumulus_pallet_parachain_system::OnSystemEvent;
use cumulus_primitives_core::PersistedValidationData;
use frame_support::{traits::ConstU32, weights::Weight};
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_parachain_primitives::primitives::HeadData;
use sp_runtime::traits::One;
use westend_runtime_constants::system_parachain::COLLECTIVES_ID;
use xcm::latest::prelude::*;

const LOG_TARGET: &str = "runtime::dday::hook";

/// Simple mechanism that syncs/sends validated Asset Hub Westend headers to other local chains.
///
/// Note: We use exactly the same crate for a Bridges scenario: https://github.com/paritytech/polkadot-sdk/pull/8326
pub type DDayHeaderSyncForCollectivesInstance = pallet_bridge_proof_root_sync::Instance1;
impl pallet_bridge_proof_root_sync::Config<DDayHeaderSyncForCollectivesInstance> for Runtime {
	type Key = BlockNumberFor<Self>;
	type Value = HeadData;
	// Both constants are 1, so we just keep/send the last
	type RootsToKeep = ConstU32<1>;
	type MaxRootsToSend = ConstU32<1>;
	type OnSend = (ToCollectivesSender,);
}

/// A type containing the encoding of the DDay detection pallet in the Collectives chain runtime.
/// Used to construct any remote calls. The codec index must correspond to the index of
/// `DDayDetection` in the `construct_runtime` of the Collectives chain.
#[derive(Encode, Decode, DecodeWithMemTracking, Debug, scale_info::TypeInfo)]
enum CollectivesPallets {
	#[codec(index = 83)]
	DDayDetection(DDayDetectionCall),
}
impl CollectivesPallets {
	fn prepare_head_call(
		block_number: BlockNumberFor<Runtime>,
		value: HeadData,
	) -> CollectivesPallets {
		CollectivesPallets::DDayDetection(DDayDetectionCall::note_new_head {
			remote_block_number: block_number,
			remote_head: value,
		})
	}
}

#[derive(Encode, Decode, DecodeWithMemTracking, Debug, scale_info::TypeInfo)]
#[allow(non_camel_case_types)]
enum DDayDetectionCall {
	#[codec(index = 0)]
	note_new_head { remote_block_number: BlockNumberFor<Runtime>, remote_head: HeadData },
}

/// Implementation of the ` OnSystemEvent ` adapter over `pallet_bridge_proof_root_sync` where we
/// can get the parent head and sync it to the Collectives.
///
/// Note: We could alternatively implement `OnSystemEvent` for `pallet_bridge_proof_root_sync`
/// directly.
pub struct DDayHeaderSyncForCollectives;
impl OnSystemEvent for DDayHeaderSyncForCollectives {
	fn on_validation_data(data: &PersistedValidationData) {
		let parent_number =
			frame_system::Pallet::<Runtime>::block_number().saturating_sub(One::one());
		DDayHook::schedule_for_sync(parent_number, data.parent_head.clone());
	}

	fn on_validation_code_applied() {}
}

/// `OnSend` implementation that sends validated AHW headers to Collectives.
pub struct ToCollectivesSender;
impl pallet_bridge_proof_root_sync::OnSend<BlockNumberFor<Runtime>, HeadData>
	for ToCollectivesSender
{
	fn on_send(roots: &Vec<(BlockNumberFor<Runtime>, HeadData)>) {
		// Prepare a call.
		let Some(dday_call) =
			roots.last().map(|(k, v)| CollectivesPallets::prepare_head_call(*k, v.clone()))
		else {
			return;
		};

		// Send dedicated `Transact` to Collectives.
		if let Err(error) = PolkadotXcm::send_xcm(
			Here,
			Location::new(1, [Parachain(COLLECTIVES_ID)]),
			Xcm::builder_unpaid()
				.unpaid_execution(Unlimited, None)
				.transact(OriginKind::Xcm, None, dday_call.encode())
				.expect_transact_status(MaybeErrorCode::Success)
				.build(),
		) {
			log::warn!(target: LOG_TARGET, "Failed to send XCM: {:?}", error);
		}
	}

	fn on_send_weight() -> Weight {
		<<Runtime as pallet_xcm::Config>::WeightInfo as pallet_xcm::WeightInfo>::send()
	}
}
