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

//! Generating test data, used by all tests.

pub mod from_grandpa_chain;
pub mod from_parachain;

use bp_messages::{
	target_chain::{DispatchMessage, DispatchMessageData},
	MessageKey,
};
use codec::Encode;
use frame_support::traits::Get;
use pallet_bridge_grandpa::BridgedHeader;
use xcm::latest::prelude::*;

use bp_messages::MessageNonce;
use bp_runtime::BasicOperatingMode;
use bp_test_utils::authority_list;
use xcm::GetVersion;
use xcm_builder::{BridgeMessage, HaulBlob, HaulBlobError, HaulBlobExporter};
use xcm_executor::traits::{validate_export, ExportXcm};

pub(crate) type XcmAsPlainPayload = sp_std::vec::Vec<u8>;

pub fn prepare_inbound_xcm(xcm_message: Xcm<()>, destination: InteriorLocation) -> Vec<u8> {
	let location = xcm::VersionedInteriorLocation::from(destination);
	let xcm = xcm::VersionedXcm::<()>::from(xcm_message);

	// (double encoding, because `.encode()` is called on original Xcm BLOB when it is pushed to the
	// storage)
	BridgeMessage { universal_dest: location, message: xcm }.encode().encode()
}

/// Helper that creates InitializationData mock data, that can be used to initialize bridge
/// GRANDPA pallet
pub fn initialization_data<
	Runtime: pallet_bridge_grandpa::Config<GrandpaPalletInstance>,
	GrandpaPalletInstance: 'static,
>(
	block_number: u32,
) -> bp_header_chain::InitializationData<BridgedHeader<Runtime, GrandpaPalletInstance>> {
	bp_header_chain::InitializationData {
		header: Box::new(bp_test_utils::test_header(block_number.into())),
		authority_list: authority_list(),
		set_id: 1,
		operating_mode: BasicOperatingMode::Normal,
	}
}

/// Dummy xcm
pub(crate) fn dummy_xcm() -> Xcm<()> {
	vec![Trap(42)].into()
}

pub(crate) fn dispatch_message<LaneId: Encode>(
	lane_id: LaneId,
	nonce: MessageNonce,
	payload: Vec<u8>,
) -> DispatchMessage<Vec<u8>, LaneId> {
	DispatchMessage {
		key: MessageKey { lane_id, nonce },
		data: DispatchMessageData { payload: Ok(payload) },
	}
}

/// Macro used for simulate_export_message and capturing bytes
macro_rules! grab_haul_blob (
	($name:ident, $grabbed_payload:ident) => {
		std::thread_local! {
			static $grabbed_payload: std::cell::RefCell<Option<Vec<u8>>> = std::cell::RefCell::new(None);
		}

		struct $name;
		impl HaulBlob for $name {
			fn haul_blob(blob: Vec<u8>) -> Result<(), HaulBlobError>{
				$grabbed_payload.with(|rm| *rm.borrow_mut() = Some(blob));
				Ok(())
			}
		}
	}
);

/// Simulates `HaulBlobExporter` and all its wrapping and captures generated plain bytes,
/// which are transferred over bridge.
pub(crate) fn simulate_message_exporter_on_bridged_chain<
	SourceNetwork: Get<NetworkId>,
	DestinationNetwork: Get<Location>,
	DestinationVersion: GetVersion,
>(
	(destination_network, destination_junctions): (NetworkId, Junctions),
) -> Vec<u8> {
	grab_haul_blob!(GrabbingHaulBlob, GRABBED_HAUL_BLOB_PAYLOAD);

	// lets pretend that some parachain on bridged chain exported the message
	let universal_source_on_bridged_chain: Junctions =
		[GlobalConsensus(SourceNetwork::get()), Parachain(5678)].into();
	let channel = 1_u32;

	// simulate XCM message export
	let (ticket, fee) = validate_export::<
		HaulBlobExporter<GrabbingHaulBlob, DestinationNetwork, DestinationVersion, ()>,
	>(
		destination_network,
		channel,
		universal_source_on_bridged_chain,
		destination_junctions,
		dummy_xcm(),
	)
	.expect("validate_export to pass");
	log::info!(
		target: "simulate_message_exporter_on_bridged_chain",
		"HaulBlobExporter::validate fee: {:?}",
		fee
	);
	let xcm_hash =
		HaulBlobExporter::<GrabbingHaulBlob, DestinationNetwork, DestinationVersion, ()>::deliver(
			ticket,
		)
		.expect("deliver to pass");
	log::info!(
		target: "simulate_message_exporter_on_bridged_chain",
		"HaulBlobExporter::deliver xcm_hash: {:?}",
		xcm_hash
	);

	GRABBED_HAUL_BLOB_PAYLOAD.with(|r| r.take().expect("Encoded message should be here"))
}
