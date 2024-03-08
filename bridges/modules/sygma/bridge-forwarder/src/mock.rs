// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

#![cfg(test)]

use cumulus_primitives_core::ParaId;

use std::marker::PhantomData;
use std::result;

use frame_support::dispatch::DispatchResult;

use frame_support::pallet_prelude::Get;

use frame_support::{
	construct_runtime, parameter_types,
	traits::{AsEnsureOriginWithArg, ConstU32},
};
use frame_system as system;
use frame_system::EnsureSigned;
use polkadot_parachain_primitives::primitives::Sibling;
use sp_runtime::testing::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};
use sp_runtime::{AccountId32, BuildStorage};
use xcm::latest::{BodyId, Junction, MultiAsset, MultiLocation, NetworkId};

use xcm::prelude::{Concrete, Fungible, GeneralKey, Parachain, X1, X3};

use sygma_traits::{AssetTypeIdentifier, Bridge, TransactorForwarder};
use xcm::v3::Weight;
use xcm_builder::{
	AccountId32Aliases, CurrencyAdapter, FungiblesAdapter, IsConcrete, NoChecking, ParentIsPreset,
	SiblingParachainConvertsVia,
};
use xcm_executor::traits::{Error as ExecutionError, MatchesFungibles};

use crate as sygma_bridge_forwarder;

construct_runtime!(
	pub struct Runtime{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Assets: pallet_assets::{Pallet, Call, Storage, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		SygmaBridgeForwarder: sygma_bridge_forwarder::{Pallet, Event<T>},
		ParachainInfo: pallet_parachain_info::{Pallet, Storage, Config<T>},
	}
);

pub(crate) type Balance = u128;

pub type AccountId = AccountId32;

type Block = frame_system::mocking::MockBlock<Runtime>;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MinimumPeriod: u64 = 1;
}

impl frame_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<2>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = ();
	type RuntimeHoldReason = ();
	type MaxHolds = ConstU32<1>;
	type MaxFreezes = ConstU32<1>;
}

parameter_types! {
	pub const AssetDeposit: Balance = 0;
	pub const AssetAccountDeposit: Balance = 0;
	pub const ApprovalDeposit: Balance = ExistentialDeposit::get();
	pub const AssetsStringLimit: u32 = 50;
	/// Key = 32 bytes, Value = 36 bytes (32+1+1+1+1)
	// https://github.com/paritytech/substrate/blob/069917b/frame/assets/src/lib.rs#L257L271
	pub const MetadataDepositBase: Balance = 0;
	pub const MetadataDepositPerByte: Balance = 0;
	pub const ExecutiveBody: BodyId = BodyId::Executive;
}

pub type AssetId = u32;

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = AssetId;
	type AssetIdParameter = codec::Compact<u32>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId32>>;
	type ForceOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = AssetAccountDeposit;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = AssetsStringLimit;
	type RemoveItemsLimit = ConstU32<1000>;
	type Freezer = ();
	type Extra = ();
	type CallbackHandle = ();
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

impl sygma_bridge_forwarder::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SygmaBridge = BridgeImplRuntime<Runtime>;
	type XCMBridge = BridgeImplRuntime<Runtime>;
}

pub struct BridgeImplRuntime<T>(PhantomData<T>);
impl<T> Bridge for BridgeImplRuntime<T> {
	fn transfer(
		_sender: [u8; 32],
		_asset: MultiAsset,
		_dest: MultiLocation,
		_max_weight: Option<Weight>,
	) -> DispatchResult {
		Ok(())
	}
}

impl pallet_parachain_info::Config for Runtime {}

pub const ALICE: AccountId32 = AccountId32::new([0u8; 32]);
pub const ASSET_OWNER: AccountId32 = AccountId32::new([1u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);
pub const ENDOWED_BALANCE: Balance = 1_000_000_000_000_000_000_000_000_000;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(ALICE, ENDOWED_BALANCE), (ASSET_OWNER, ENDOWED_BALANCE)],
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

#[allow(dead_code)]
pub fn assert_events(mut expected: Vec<RuntimeEvent>) {
	let mut actual: Vec<RuntimeEvent> =
		system::Pallet::<Runtime>::events().iter().map(|e| e.event.clone()).collect();

	expected.reverse();

	for evt in expected {
		let next = actual.pop().expect("event expected");
		assert_eq!(next, evt, "Events don't match");
	}
}

// mock the generic types of XCMAssetTransactor
parameter_types! {
	pub NativeLocation: MultiLocation = MultiLocation::here();
	pub UsdtAssetId: AssetId = 1;
	pub UsdtLocation: MultiLocation = MultiLocation::new(
		1,
		X3(
			Parachain(2005),
			slice_to_generalkey(b"sygma"),
			slice_to_generalkey(b"usdt"),
		),
	);
	pub CheckingAccount: AccountId32 = AccountId32::new([102u8; 32]);

	pub const RelayNetwork: NetworkId = NetworkId::Rococo;
}

pub type CurrencyTransactor = CurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<NativeLocation>,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId32,
	// We don't track any teleports of `Balances`.
	(),
>;

pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	Assets,
	// Use this currency when it is a fungible asset matching the given location or name:
	SimpleForeignAssetConverter,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId32,
	// Disable teleport.
	NoChecking,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

pub type LocationToAccountId = (
	ParentIsPreset<AccountId>,
	SiblingParachainConvertsVia<Sibling, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
);

pub struct SimpleForeignAssetConverter(PhantomData<()>);
impl MatchesFungibles<AssetId, Balance> for SimpleForeignAssetConverter {
	fn matches_fungibles(a: &MultiAsset) -> result::Result<(AssetId, Balance), ExecutionError> {
		match (&a.fun, &a.id) {
			(Fungible(ref amount), Concrete(ref id)) => {
				if id == &UsdtLocation::get() {
					Ok((UsdtAssetId::get(), *amount))
				} else {
					Err(ExecutionError::AssetNotHandled)
				}
			},
			_ => Err(ExecutionError::AssetNotHandled),
		}
	}
}

/// NativeAssetTypeIdentifier impl AssetTypeIdentifier for XCMAssetTransactor
/// This impl is only for local mock purpose, the integrated parachain might have their own version
pub struct NativeAssetTypeIdentifier<T>(PhantomData<T>);
impl<T: Get<ParaId>> AssetTypeIdentifier for NativeAssetTypeIdentifier<T> {
	/// check if the given MultiAsset is a native asset
	fn is_native_asset(asset: &MultiAsset) -> bool {
		// currently there are two multilocations are considered as native asset:
		// 1. integrated parachain native asset(MultiLocation::here())
		// 2. other parachain native asset(MultiLocation::new(1, X1(Parachain(T::get().into()))))
		let native_locations =
			[MultiLocation::here(), MultiLocation::new(1, X1(Parachain(T::get().into())))];

		match (&asset.id, &asset.fun) {
			(Concrete(ref id), Fungible(_)) => native_locations.contains(id),
			_ => false,
		}
	}
}

pub fn slice_to_generalkey(key: &[u8]) -> Junction {
	let len = key.len();
	assert!(len <= 32);
	GeneralKey {
		length: len as u8,
		data: {
			let mut data = [0u8; 32];
			data[..len].copy_from_slice(key);
			data
		},
	}
}

pub struct ForwarderImplRuntime;

impl TransactorForwarder for ForwarderImplRuntime {
	fn xcm_transactor_forwarder(
		_sender: [u8; 32],
		_what: MultiAsset,
		_dest: MultiLocation,
	) -> DispatchResult {
		Ok(())
	}

	fn other_world_transactor_forwarder(
		_sender: [u8; 32],
		_what: MultiAsset,
		_dest: MultiLocation,
	) -> DispatchResult {
		Ok(())
	}
}
