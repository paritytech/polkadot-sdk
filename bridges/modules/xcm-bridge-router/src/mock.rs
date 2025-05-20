// Copyright (C) Parity Technologies (UK) Ltd.
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

#![cfg(test)]

use crate as pallet_xcm_bridge_router;

use crate::impls::EnsureIsRemoteBridgeIdResolver;
use bp_xcm_bridge_router::{BridgeState, ResolveBridgeId};
use codec::Encode;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{Contains, Equals},
};
use frame_system::EnsureRoot;
use sp_runtime::{traits::ConstU128, BuildStorage};
use sp_std::cell::RefCell;
use xcm::prelude::*;
use xcm_builder::{
	InspectMessageQueues, NetworkExportTable, NetworkExportTableItem, SovereignPaidRemoteExporter,
};

type Block = frame_system::mocking::MockBlock<TestRuntime>;

/// HRMP fee.
pub const HRMP_FEE: u128 = 500;
/// Base bridge fee.
pub const BASE_FEE: u128 = 1_000_000;
/// Byte bridge fee.
pub const BYTE_FEE: u128 = 1_000;

construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system,
		XcmBridgeHubRouter: pallet_xcm_bridge_router,
	}
}

parameter_types! {
	pub ThisNetworkId: NetworkId = Polkadot;
	pub BridgedNetworkId: NetworkId = Kusama;
	pub UniversalLocation: InteriorLocation = [GlobalConsensus(ThisNetworkId::get()), Parachain(1000)].into();
	pub SiblingBridgeHubLocation: Location = ParentThen([Parachain(1002)].into()).into();
	pub BridgeFeeAsset: AssetId = Location::parent().into();
	pub BridgeTable: Vec<NetworkExportTableItem>
		= vec![
			NetworkExportTableItem::new(
				BridgedNetworkId::get(),
				None,
				SiblingBridgeHubLocation::get(),
				Some((BridgeFeeAsset::get(), BASE_FEE).into())
			)
		];
	pub UnknownXcmVersionForRoutableLocation: Location = Location::new(2, [GlobalConsensus(BridgedNetworkId::get()), Parachain(9999)]);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
}

/// Simple implementation where every dest resolves to the exact one `BridgeId`.
pub struct EveryDestinationToSameBridgeIdResolver;
impl ResolveBridgeId for EveryDestinationToSameBridgeIdResolver {
	type BridgeId = ();

	fn resolve_for_dest(_dest: &Location) -> Option<Self::BridgeId> {
		Some(())
	}

	fn resolve_for(
		_bridged_network: &NetworkId,
		_bridged_dest: &InteriorLocation,
	) -> Option<Self::BridgeId> {
		Some(())
	}
}

/// An instance of `pallet_xcm_bridge_hub_router` configured to use a remote exporter with the
/// `ExportMessage` instruction, which will be delivered to a sibling parachain using
/// `SiblingBridgeHubLocation`.
#[derive_impl(pallet_xcm_bridge_router::config_preludes::TestDefaultConfig)]
impl pallet_xcm_bridge_router::Config<()> for TestRuntime {
	type DestinationVersion =
		LatestOrNoneForLocationVersionChecker<Equals<UnknownXcmVersionForRoutableLocation>>;

	type MessageExporter = SovereignPaidRemoteExporter<
		pallet_xcm_bridge_router::impls::ViaRemoteBridgeExporter<
			TestRuntime,
			(),
			NetworkExportTable<BridgeTable>,
			BridgedNetworkId,
			SiblingBridgeHubLocation,
		>,
		TestXcmRouter,
		UniversalLocation,
	>;

	type BridgeIdResolver = EnsureIsRemoteBridgeIdResolver<UniversalLocation>;
	type UpdateBridgeStatusOrigin = EnsureRoot<u64>;

	type ByteFee = ConstU128<BYTE_FEE>;
	type FeeAsset = BridgeFeeAsset;
}

pub struct LatestOrNoneForLocationVersionChecker<Location>(sp_std::marker::PhantomData<Location>);
impl<LocationValue: Contains<Location>> GetVersion
	for LatestOrNoneForLocationVersionChecker<LocationValue>
{
	fn get_version_for(dest: &Location) -> Option<XcmVersion> {
		if LocationValue::contains(dest) {
			return None
		}
		Some(XCM_VERSION)
	}
}

pub struct TestXcmRouter;

impl TestXcmRouter {
	pub fn is_message_sent() -> bool {
		!Self::get_messages().is_empty()
	}
}

thread_local! {
	pub static SENT_XCM: RefCell<Vec<(Location, Xcm<()>)>> = RefCell::new(Vec::new());
}

impl SendXcm for TestXcmRouter {
	type Ticket = (Location, Xcm<()>);

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let pair = (destination.take().unwrap(), message.take().unwrap());
		Ok((pair, (BridgeFeeAsset::get(), HRMP_FEE).into()))
	}

	fn deliver(pair: Self::Ticket) -> Result<XcmHash, SendError> {
		let hash = fake_message_hash(&pair.1);
		SENT_XCM.with(|q| q.borrow_mut().push(pair));
		Ok(hash)
	}
}

impl InspectMessageQueues for TestXcmRouter {
	fn clear_messages() {
		SENT_XCM.with(|q| q.borrow_mut().clear());
	}

	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		SENT_XCM.with(|q| {
			(*q.borrow())
				.clone()
				.iter()
				.map(|(location, message)| {
					(
						VersionedLocation::from(location.clone()),
						vec![VersionedXcm::from(message.clone())],
					)
				})
				.collect()
		})
	}
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
	sp_io::TestExternalities::new(t)
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		System::reset_events();

		test()
	})
}

pub(crate) fn fake_message_hash<T>(message: &Xcm<T>) -> XcmHash {
	message.using_encoded(sp_io::hashing::blake2_256)
}

pub(crate) fn set_bridge_state_for<T: pallet_xcm_bridge_router::Config<I>, I: 'static>(
	dest: &Location,
	bridge_state: Option<BridgeState>,
) -> pallet_xcm_bridge_router::BridgeIdOf<T, I> {
	let bridge_id = <T::BridgeIdResolver as ResolveBridgeId>::resolve_for_dest(dest).unwrap();
	if let Some(bridge_state) = bridge_state {
		pallet_xcm_bridge_router::Bridges::<T, I>::insert(&bridge_id, bridge_state);
	} else {
		pallet_xcm_bridge_router::Bridges::<T, I>::remove(&bridge_id);
	}
	bridge_id
}

pub(crate) fn get_bridge_state_for<T: pallet_xcm_bridge_router::Config<I>, I: 'static>(
	dest: &Location,
) -> Option<BridgeState> {
	let bridge_id = <T::BridgeIdResolver as ResolveBridgeId>::resolve_for_dest(dest).unwrap();
	pallet_xcm_bridge_router::Bridges::<T, I>::get(bridge_id)
}

#[cfg(feature = "runtime-benchmarks")]
impl crate::benchmarking::Config<()> for TestRuntime {
	fn ensure_bridged_target_destination() -> Result<Location, frame_benchmarking::BenchmarkError> {
		Ok(Location::new(2, [GlobalConsensus(BridgedNetworkId::get())]))
	}
	fn update_bridge_status_origin() -> Option<RuntimeOrigin> {
		Some(RuntimeOrigin::root())
	}
}
