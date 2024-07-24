// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use crate::test_utils::TrappedAssets;

#[test]
fn sovereign_paid_remote_exporter_produces_xcm_which_does_not_trap_assets() {
	frame_support::parameter_types! {
		pub BridgeFeeAsset: Location = Parent.into();
		pub LocalNetwork: NetworkId = ExecutorUniversalLocation::get().global_consensus().expect("valid `NetworkId`");
		pub LocalBridgeLocation: Location = match &ExecutorUniversalLocation::get().split_global() {
			Ok((_, junctions)) => Location::new(1, junctions.clone()),
			_ => panic!("unexpected location format")
		};
		pub RemoteNetwork: NetworkId = ByGenesis([1; 32]);
		pub SendOverBridgePrice: u128 = 333;
		pub BridgeTable: Vec<NetworkExportTableItem> = vec![
			NetworkExportTableItem::new(
				RemoteNetwork::get(),
				None,
				LocalBridgeLocation::get(),
				Some((BridgeFeeAsset::get(), SendOverBridgePrice::get()).into())
			)
		];
		pub static SenderUniversalLocation: InteriorLocation = (LocalNetwork::get(), Parachain(50)).into();
	}

	// `SovereignPaidRemoteExporter` e.g. used on sibling of `ExecutorUniversalLocation`
	type Exporter = SovereignPaidRemoteExporter<
		NetworkExportTable<BridgeTable>,
		TestMessageSender,
		SenderUniversalLocation,
	>;

	// prepare message on sending chain with tested `Exporter` and translate it to the executor
	// message type
	let message = Exporter::validate(
		&mut Some(Location::new(2, [GlobalConsensus(RemoteNetwork::get())])),
		&mut Some(Xcm(vec![])),
	)
	.expect("valid message");
	let message = Xcm::<TestCall>::from(message.0 .1);
	let mut message_id = message.using_encoded(sp_io::hashing::blake2_256);

	// allow origin to pass barrier
	let origin = Location::new(1, Parachain(50));
	AllowPaidFrom::set(vec![origin.clone()]);

	// fund origin
	add_asset(origin.clone(), (AssetId(BridgeFeeAsset::get()), SendOverBridgePrice::get() * 2));
	WeightPrice::set((BridgeFeeAsset::get().into(), 1_000_000_000_000, 1024 * 1024));

	// check before
	assert!(TrappedAssets::get().is_empty());
	assert_eq!(exported_xcm(), vec![]);

	// execute XCM with overrides for `MessageExporter` behavior to return `Unroutable` error on
	// validate
	set_exporter_override(
		|_, _, _, _, _| Err(SendError::Unroutable),
		|_, _, _, _, _| Err(SendError::Transport("not allowed to call here")),
	);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		origin.clone(),
		message.clone(),
		&mut message_id,
		Weight::from_parts(2_000_000_000_000, 2_000_000_000_000),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(50, 50), error: XcmError::Unroutable }
	);
	// check empty trapped assets
	assert!(TrappedAssets::get().is_empty());
	// no xcm exported
	assert_eq!(exported_xcm(), vec![]);

	// execute XCM again with clear `MessageExporter` overrides behavior to expect delivery
	clear_exporter_override();
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		origin.clone(),
		message,
		&mut message_id,
		Weight::from_parts(2_000_000_000_000, 2_000_000_000_000),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(50, 50) });

	// check empty trapped assets
	assert!(TrappedAssets::get().is_empty());
	// xcm exported
	assert_eq!(exported_xcm().len(), 1);
}
