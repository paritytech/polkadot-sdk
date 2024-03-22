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

//! A mock runtime for XCM benchmarking.

use crate::{fungible as xcm_balances_benchmark, mock::*};
use frame_benchmarking::BenchmarkError;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, Everything, Nothing},
	weights::Weight,
};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};
use xcm::latest::prelude::*;
use xcm_builder::{AllowUnpaidExecutionFrom, FrameTransactionalProcessor, MintLocation};

type Block = frame_system::mocking::MockBlock<Test>;

// For testing the pallet, we construct a mock runtime.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		XcmBalancesBenchmark: xcm_balances_benchmark,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(Weight::from_parts(1024, u64::MAX));
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type Nonce = u64;
	type Hash = H256;
	type RuntimeCall = RuntimeCall;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 7;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

parameter_types! {
	pub const AssetDeposit: u64 = 100 * ExistentialDeposit::get();
	pub const ApprovalDeposit: u64 = 1 * ExistentialDeposit::get();
	pub const StringLimit: u32 = 50;
	pub const MetadataDepositBase: u64 = 10 * ExistentialDeposit::get();
	pub const MetadataDepositPerByte: u64 = 1 * ExistentialDeposit::get();
}

pub struct MatchAnyFungible;
impl xcm_executor::traits::MatchesFungible<u64> for MatchAnyFungible {
	fn matches_fungible(m: &Asset) -> Option<u64> {
		use sp_runtime::traits::SaturatedConversion;
		match m {
			Asset { fun: Fungible(amount), .. } => Some((*amount).saturated_into::<u64>()),
			_ => None,
		}
	}
}

// Use balances as the asset transactor.
pub type AssetTransactor = xcm_builder::FungibleAdapter<
	Balances,
	MatchAnyFungible,
	AccountIdConverter,
	u64,
	CheckingAccount,
>;

parameter_types! {
	/// Maximum number of instructions in a single XCM fragment. A sanity check against weight
	/// calculations getting too crazy.
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = DevNull;
	type AssetTransactor = AssetTransactor;
	type OriginConverter = ();
	type IsReserve = TrustedReserves;
	type IsTeleporter = TrustedTeleporters;
	type UniversalLocation = UniversalLocation;
	type Barrier = AllowUnpaidExecutionFrom<Everything>;
	type Weigher = xcm_builder::FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader = xcm_builder::FixedRateOfFungible<WeightPrice, ()>;
	type ResponseHandler = DevNull;
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
}

impl crate::Config for Test {
	type XcmConfig = XcmConfig;
	type AccountIdConverter = AccountIdConverter;
	type DeliveryHelper = ();
	fn valid_destination() -> Result<Location, BenchmarkError> {
		let valid_destination: Location = [AccountId32 { network: None, id: [0u8; 32] }].into();

		Ok(valid_destination)
	}
	fn worst_case_holding(depositable_count: u32) -> Assets {
		crate::mock_worst_case_holding(
			depositable_count,
			<XcmConfig as xcm_executor::Config>::MaxAssetsIntoHolding::get(),
		)
	}
}

pub type TrustedTeleporters = xcm_builder::Case<TeleportConcreteFungible>;
pub type TrustedReserves = xcm_builder::Case<ReserveConcreteFungible>;

parameter_types! {
	pub const CheckingAccount: Option<(u64, MintLocation)> = Some((100, MintLocation::Local));
	pub ChildTeleporter: Location = Parachain(1000).into_location();
	pub TrustedTeleporter: Option<(Location, Asset)> = Some((
		ChildTeleporter::get(),
		Asset { id: AssetId(Here.into_location()), fun: Fungible(100) },
	));
	pub TrustedReserve: Option<(Location, Asset)> = Some((
		ChildTeleporter::get(),
		Asset { id: AssetId(Here.into_location()), fun: Fungible(100) },
	));
	pub TeleportConcreteFungible: (AssetFilter, Location) =
		(Wild(AllOf { fun: WildFungible, id: AssetId(Here.into_location()) }), ChildTeleporter::get());
	pub ReserveConcreteFungible: (AssetFilter, Location) =
		(Wild(AllOf { fun: WildFungible, id: AssetId(Here.into_location()) }), ChildTeleporter::get());
}

impl xcm_balances_benchmark::Config for Test {
	type TransactAsset = Balances;
	type CheckedAccount = CheckingAccount;
	type TrustedTeleporter = TrustedTeleporter;
	type TrustedReserve = TrustedReserve;

	fn get_asset() -> Asset {
		let amount = 1_000_000_000_000;
		Asset { id: AssetId(Here.into()), fun: Fungible(amount) }
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub fn new_test_ext() -> sp_io::TestExternalities {
	use sp_runtime::BuildStorage;
	let t = RuntimeGenesisConfig { ..Default::default() }.build_storage().unwrap();
	sp_tracing::try_init_simple();
	t.into()
}
