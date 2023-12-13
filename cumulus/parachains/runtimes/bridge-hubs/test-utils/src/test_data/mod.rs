// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Generating test data, used by all tests.

pub mod from_grandpa_chain;
pub mod from_parachain;

use bp_messages::{
	target_chain::{DispatchMessage, DispatchMessageData},
	LaneId, MessageKey,
};
use codec::Encode;
use frame_support::traits::Get;
use pallet_bridge_grandpa::BridgedHeader;
use xcm::latest::prelude::*;

use bp_messages::MessageNonce;
use bp_runtime::BasicOperatingMode;
use bp_test_utils::authority_list;
use xcm::GetVersion;
use xcm_builder::{HaulBlob, HaulBlobError, HaulBlobExporter};
use xcm_executor::traits::{validate_export, ExportXcm};

pub fn prepare_inbound_xcm<InnerXcmRuntimeCall>(
	xcm_message: Xcm<InnerXcmRuntimeCall>,
	destination: InteriorLocation,
) -> Vec<u8> {
	let location = xcm::VersionedInteriorLocation::V4(destination);
	let xcm = xcm::VersionedXcm::<InnerXcmRuntimeCall>::V4(xcm_message);
	// this is the `BridgeMessage` from polkadot xcm builder, but it has no constructor
	// or public fields, so just tuple
	// (double encoding, because `.encode()` is called on original Xcm BLOB when it is pushed
	// to the storage)
	(location, xcm).encode().encode()
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

pub(crate) fn dispatch_message(
	lane_id: LaneId,
	nonce: MessageNonce,
	payload: Vec<u8>,
) -> DispatchMessage<Vec<u8>> {
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
