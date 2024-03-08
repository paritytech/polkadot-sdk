// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

#![cfg(test)]

use frame_support::{
	pallet_prelude::ConstU32,
	parameter_types,
	sp_runtime::{
		testing::H256,
		traits::{BlakeTwo256, IdentityLookup},
		AccountId32, BuildStorage, Perbill,
	},
	traits::{AsEnsureOriginWithArg, ConstU128},
};
use frame_system::{self as system, EnsureRoot, EnsureSigned};
use sygma_traits::DomainID;
use xcm::latest::MultiLocation;

use crate as fee_handler_router;

type Block = frame_system::mocking::MockBlock<Test>;

pub(crate) type Balance = u128;

pub const ALICE: AccountId32 = AccountId32::new([0u8; 32]);

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Assets: pallet_assets::{Pallet, Call, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		AccessSegregator: sygma_access_segregator::{Pallet, Call, Storage, Event<T>} = 3,
		SygmaBasicFeeHandler: sygma_basic_feehandler::{Pallet, Call, Storage, Event<T>} = 4,
		FeeHandlerRouter: fee_handler_router::{Pallet, Call, Storage, Event<T>} = 5,
		SygamPercenrageFeeHandler: sygma_percentage_feehandler::{Pallet, Call, Storage, Event<T>} = 6,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
	pub const MaxLocks: u32 = 100;
	pub const MinimumPeriod: u64 = 1;
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type Block = Block;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
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

impl pallet_balances::Config for Test {
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
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type MaxHolds = ();
}

parameter_types! {
	pub const AssetDeposit: Balance = 1; // 1 Unit deposit to create asset
	pub const ApprovalDeposit: Balance = 1;
	pub const AssetsStringLimit: u32 = 50;
	pub const MetadataDepositBase: Balance = 1;
	pub const MetadataDepositPerByte: Balance = 1;
}

impl pallet_assets::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = u32;
	type AssetIdParameter = codec::Compact<u32>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId32>>;
	type ForceOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = ConstU128<10>;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = AssetsStringLimit;
	type RemoveItemsLimit = ConstU32<1000>;
	type Freezer = ();
	type Extra = ();
	type CallbackHandle = ();
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Test>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

parameter_types! {
	pub const EthereumDomainID: DomainID = 0;
	pub const MoonbeamDomainID: DomainID = 1;
	// Make sure put same value with `construct_runtime`
	pub const AccessSegregatorPalletIndex: u8 = 3;
	pub const BasicFeeHandlerPalletIndex: u8 = 4;
	pub const FeeHandlerRouterPalletIndex: u8 = 5;
	pub const PercentageFeeHandlerPalletIndex: u8 = 6;
	pub RegisteredExtrinsics: Vec<(u8, Vec<u8>)> = [
		(AccessSegregatorPalletIndex::get(), b"grant_access".to_vec()),
		(FeeHandlerRouterPalletIndex::get(), b"set_fee_handler".to_vec()),
	].to_vec();
	pub PhaLocation: MultiLocation = MultiLocation::here();
}

impl sygma_basic_feehandler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type PalletIndex = BasicFeeHandlerPalletIndex;
	type WeightInfo = sygma_basic_feehandler::weights::SygmaWeightInfo<Test>;
}

impl sygma_access_segregator::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BridgeCommitteeOrigin = EnsureRoot<Self::AccountId>;
	type PalletIndex = AccessSegregatorPalletIndex;
	type Extrinsics = RegisteredExtrinsics;
	type WeightInfo = sygma_access_segregator::weights::SygmaWeightInfo<Test>;
}

impl fee_handler_router::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BasicFeeHandler = SygmaBasicFeeHandler;
	type DynamicFeeHandler = ();
	type PercentageFeeHandler = SygamPercenrageFeeHandler;
	type PalletIndex = FeeHandlerRouterPalletIndex;
	type WeightInfo = fee_handler_router::weights::SygmaWeightInfo<Test>;
}

impl sygma_percentage_feehandler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type PalletIndex = PercentageFeeHandlerPalletIndex;
	type WeightInfo = sygma_percentage_feehandler::weights::SygmaWeightInfo<Test>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

// Checks events against the latest. A contiguous set of events must be provided. They must
// include the most recent event, but do not have to include every past event.
pub fn assert_events(mut expected: Vec<RuntimeEvent>) {
	let mut actual: Vec<RuntimeEvent> =
		system::Pallet::<Test>::events().iter().map(|e| e.event.clone()).collect();

	expected.reverse();

	for evt in expected {
		let next = actual.pop().expect("event expected");
		assert_eq!(next, evt, "Events don't match");
	}
}
