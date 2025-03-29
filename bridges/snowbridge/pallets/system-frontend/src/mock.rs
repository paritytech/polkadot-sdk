// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate as snowbridge_system_frontend;
#[cfg(feature = "runtime-benchmarks")]
use crate::BenchmarkHelper;
use frame_support::{
	derive_impl, parameter_types,
	traits::{AsEnsureOriginWithArg, Everything},
};
pub use snowbridge_test_utils::{mock_origin::pallet_xcm_origin, mock_xcm::*};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	AccountId32, BuildStorage,
};
use xcm::prelude::*;

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = AccountId32;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		XcmOrigin: pallet_xcm_origin::{Pallet, Origin},
		EthereumSystemFrontend: snowbridge_system_frontend,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = RuntimeTask;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u128>;
	type Nonce = u64;
	type Block = Block;
}

impl pallet_xcm_origin::Config for Test {
	type RuntimeOrigin = RuntimeOrigin;
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper<RuntimeOrigin> for () {
	fn make_xcm_origin(location: Location) -> RuntimeOrigin {
		RuntimeOrigin::from(pallet_xcm_origin::Origin(location))
	}

	fn initialize_storage(_: Location, _: Location) {}
}

parameter_types! {
	pub storage Ether: Location = Location::new(
				2,
				[
					GlobalConsensus(Ethereum { chain_id: 11155111 }),
				],
	);
	pub storage DeliveryFee: Asset = (Location::parent(), 80_000_000_000u128).into();
	pub BridgeHubLocation: Location = Location::new(1, [Parachain(1002)]);
	pub UniversalLocation: InteriorLocation =
		[GlobalConsensus(Polkadot), Parachain(1000)].into();
	pub PalletLocation: InteriorLocation = [PalletInstance(36)].into();
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RegisterTokenOrigin = AsEnsureOriginWithArg<pallet_xcm_origin::EnsureXcm<Everything>>;
	type XcmSender = MockXcmSender;
	type AssetTransactor = SuccessfulTransactor;
	type EthereumLocation = Ether;
	type XcmExecutor = MockXcmExecutor;
	type BridgeHubLocation = BridgeHubLocation;
	type UniversalLocation = UniversalLocation;
	type PalletLocation = PalletLocation;
	type BackendWeightInfo = ();
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

pub fn make_xcm_origin(location: Location) -> RuntimeOrigin {
	pallet_xcm_origin::Origin(location).into()
}
