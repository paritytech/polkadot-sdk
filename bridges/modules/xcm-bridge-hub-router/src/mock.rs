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

use crate as pallet_xcm_bridge_hub_router;

use bp_xcm_bridge_hub_router::XcmChannelStatusProvider;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{Contains, Equals},
};
use frame_system::EnsureRoot;
use sp_runtime::{traits::ConstU128, BuildStorage};
use xcm::prelude::*;
use xcm_builder::{NetworkExportTable, NetworkExportTableItem};

pub type AccountId = u64;
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
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		XcmBridgeHubRouter: pallet_xcm_bridge_hub_router::{Pallet, Storage},
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
	pub UnknownXcmVersionLocation: Location = Location::new(2, [GlobalConsensus(BridgedNetworkId::get()), Parachain(9999)]);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
}

impl pallet_xcm_bridge_hub_router::Config<()> for TestRuntime {
	type WeightInfo = ();

	type UniversalLocation = UniversalLocation;
	type BridgedNetworkId = BridgedNetworkId;
	type Bridges = NetworkExportTable<BridgeTable>;
	type DestinationVersion =
		LatestOrNoneForLocationVersionChecker<Equals<UnknownXcmVersionLocation>>;

	type BridgeHubOrigin = EnsureRoot<AccountId>;
	type ToBridgeHubSender = TestToBridgeHubSender;
	type WithBridgeHubChannel = TestWithBridgeHubChannel;

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

pub struct TestToBridgeHubSender;

impl TestToBridgeHubSender {
	pub fn is_message_sent() -> bool {
		frame_support::storage::unhashed::get_or_default(b"TestToBridgeHubSender.Sent")
	}
}

impl SendXcm for TestToBridgeHubSender {
	type Ticket = ();

	fn validate(
		_destination: &mut Option<Location>,
		_message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		Ok(((), (BridgeFeeAsset::get(), HRMP_FEE).into()))
	}

	fn deliver(_ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		frame_support::storage::unhashed::put(b"TestToBridgeHubSender.Sent", &true);
		Ok([0u8; 32])
	}
}

pub struct TestWithBridgeHubChannel;

impl TestWithBridgeHubChannel {
	pub fn make_congested() {
		frame_support::storage::unhashed::put(b"TestWithBridgeHubChannel.Congested", &true);
	}
}

impl XcmChannelStatusProvider for TestWithBridgeHubChannel {
	fn is_congested() -> bool {
		frame_support::storage::unhashed::get_or_default(b"TestWithBridgeHubChannel.Congested")
	}
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
	sp_io::TestExternalities::new(t)
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(test)
}
