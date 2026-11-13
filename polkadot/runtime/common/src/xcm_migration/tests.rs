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

//! Tests of [`MigrateXcmToLatest`].

use super::*;
#[cfg(feature = "runtime-benchmarks")]
use crate::impls::benchmarks::{AssetRateArguments, TreasuryArguments};
use frame_support::{
	derive_impl, parameter_types,
	traits::{
		tokens::{Pay, PayWithSource, PaymentStatus, UnityAssetBalanceConversion},
		NeverEnsureOrigin,
	},
	weights::Weight,
	PalletId, StorageHasher,
};
use pallet_asset_rate::ConversionRateToNative;
use pallet_multi_asset_bounties::{Bounties, Bounty, BountyStatus};
use pallet_treasury::{PaymentState, SpendStatus, Spends};
use sp_runtime::{traits::IdentityLookup, BuildStorage, FixedU128};
use xcm::{latest::prelude::*, v3::Junctions as V3Junctions};

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
type Balance = u64;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Preimage: pallet_preimage,
		AssetRate: pallet_asset_rate,
		Treasury: pallet_treasury,
		MultiAssetBounties: pallet_multi_asset_bounties,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type ManagerOrigin = frame_system::EnsureRoot<AccountId>;
	type Consideration = ();
}

impl pallet_asset_rate::Config for Test {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type CreateOrigin = frame_system::EnsureRoot<AccountId>;
	type RemoveOrigin = frame_system::EnsureRoot<AccountId>;
	type UpdateOrigin = frame_system::EnsureRoot<AccountId>;
	type Currency = Balances;
	type AssetKind = VersionedLocatableAsset;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = AssetRateArguments;
}

/// A paymaster that is never meant to be called.
pub struct NoPay;
impl Pay for NoPay {
	type Balance = Balance;
	type Beneficiary = VersionedLocation;
	type AssetKind = VersionedLocatableAsset;
	type Id = u64;
	type Error = ();
	fn pay(_: &VersionedLocation, _: VersionedLocatableAsset, _: Balance) -> Result<u64, ()> {
		Err(())
	}
	fn check_payment(_: u64) -> PaymentStatus {
		PaymentStatus::Unknown
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &VersionedLocation, _: VersionedLocatableAsset, _: Balance) {}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: u64) {}
}
impl PayWithSource for NoPay {
	type Balance = Balance;
	type Source = VersionedLocation;
	type Beneficiary = VersionedLocation;
	type AssetKind = VersionedLocatableAsset;
	type Id = u64;
	type Error = ();
	fn pay(
		_: &VersionedLocation,
		_: &VersionedLocation,
		_: VersionedLocatableAsset,
		_: Balance,
	) -> Result<u64, ()> {
		Err(())
	}
	fn check_payment(_: u64) -> PaymentStatus {
		PaymentStatus::Unknown
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		_: &VersionedLocation,
		_: &VersionedLocation,
		_: VersionedLocatableAsset,
		_: Balance,
	) {
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: u64) {}
}

parameter_types! {
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
}

impl pallet_treasury::Config for Test {
	type Currency = Balances;
	type RejectOrigin = frame_system::EnsureRoot<AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = ();
	type Burn = ();
	type BurnDestination = ();
	type PalletId = TreasuryPalletId;
	type SpendFunds = ();
	type MaxApprovals = ConstU32<100>;
	type WeightInfo = ();
	type SpendOrigin = NeverEnsureOrigin<Balance>;
	type AssetKind = VersionedLocatableAsset;
	type Beneficiary = VersionedLocation;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type Paymaster = NoPay;
	type BalanceConverter = UnityAssetBalanceConversion;
	type PayoutPeriod = ();
	type BlockNumberProvider = System;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = TreasuryArguments;
}

/// Converts nothing, as no payment is ever made.
pub struct NoSource;
impl<T> TryConvert<T, VersionedLocation> for NoSource {
	fn try_convert(value: T) -> Result<VersionedLocation, T> {
		Err(value)
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BountiesArguments;
#[cfg(feature = "runtime-benchmarks")]
impl
	pallet_multi_asset_bounties::ArgumentsFactory<
		VersionedLocatableAsset,
		VersionedLocation,
		Balance,
	> for BountiesArguments
{
	fn create_asset_kind(seed: u32) -> VersionedLocatableAsset {
		<TreasuryArguments as pallet_treasury::ArgumentsFactory<_, _>>::create_asset_kind(seed)
	}
	fn create_beneficiary(seed: [u8; 32]) -> VersionedLocation {
		<TreasuryArguments as pallet_treasury::ArgumentsFactory<_, _>>::create_beneficiary(seed)
	}
}

impl pallet_multi_asset_bounties::Config for Test {
	type Balance = Balance;
	type RejectOrigin = frame_system::EnsureRoot<AccountId>;
	type SpendOrigin = NeverEnsureOrigin<Balance>;
	type AssetKind = VersionedLocatableAsset;
	type Beneficiary = VersionedLocation;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type BountyValueMinimum = ();
	type ChildBountyValueMinimum = ();
	type MaxActiveChildBountyCount = ();
	type WeightInfo = ();
	type FundingSource = NoSource;
	type BountySource = NoSource;
	type ChildBountySource = NoSource;
	type Paymaster = NoPay;
	type BalanceConverter = UnityAssetBalanceConversion;
	type Preimages = Preimage;
	type Consideration = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BountiesArguments;
}

/// Weights that make the number of entries upgraded per step easy to control.
pub struct TestWeights;
impl WeightInfo for TestWeights {
	fn step_base() -> Weight {
		Weight::from_parts(10, 10)
	}
	fn step_item() -> Weight {
		Weight::from_parts(100, 100)
	}
}

pub(crate) type AssetRateMigration =
	AssetRateStage<Test, VersionedLocatableAssetToLatest, TestWeights>;
type TreasuryMigration = TreasurySpendsStage<
	Test,
	(),
	VersionedLocatableAssetToLatest,
	VersionedLocationToLatest,
	TestWeights,
>;
type BountiesMigration = BountiesStage<Test, (), VersionedLocatableAssetToLatest, TestWeights>;
type Migration = MigrateXcmToLatest<(AssetRateMigration, TreasuryMigration, BountiesMigration)>;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

fn v3_asset(para_id: u32, general_index: u128) -> VersionedLocatableAsset {
	use xcm::v3::Junction::*;
	VersionedLocatableAsset::V3 {
		location: xcm::v3::Location::new(0, V3Junctions::X1(Parachain(para_id))),
		asset_id: xcm::v3::AssetId::Concrete(xcm::v3::Location::new(
			0,
			V3Junctions::X2(PalletInstance(50), GeneralIndex(general_index)),
		)),
	}
}

fn latest_asset(para_id: u32, general_index: u128) -> VersionedLocatableAsset {
	(
		Location::new(0, [Parachain(para_id)]),
		AssetId(Location::new(0, [PalletInstance(50), GeneralIndex(general_index)])),
	)
		.into()
}

/// Can't be upgraded, as abstract asset ids don't exist after XCM v3.
fn abstract_asset() -> VersionedLocatableAsset {
	VersionedLocatableAsset::V3 {
		location: xcm::v3::Location::here(),
		asset_id: xcm::v3::AssetId::Abstract([1; 32]),
	}
}

fn v3_beneficiary(account: u8) -> VersionedLocation {
	VersionedLocation::V3(xcm::v3::Location::new(
		0,
		V3Junctions::X1(xcm::v3::Junction::AccountId32 { network: None, id: [account; 32] }),
	))
}

fn latest_beneficiary(account: u8) -> VersionedLocation {
	Location::new(0, [AccountId32 { network: None, id: [account; 32] }]).into()
}

fn spend(
	asset_kind: VersionedLocatableAsset,
	beneficiary: VersionedLocation,
) -> SpendStatus<VersionedLocatableAsset, Balance, VersionedLocation, u64, u64> {
	SpendStatus {
		asset_kind,
		amount: 10,
		beneficiary,
		valid_from: 1,
		expire_at: 10,
		status: PaymentState::Pending,
	}
}

fn bounty(asset_kind: VersionedLocatableAsset) -> pallet_multi_asset_bounties::BountyOf<Test, ()> {
	Bounty {
		asset_kind,
		value: 10,
		metadata: Default::default(),
		status: BountyStatus::Funded { curator: 4 },
	}
}

/// Fills storage with entries covering every case, returning a check of the expected result.
fn setup() -> impl Fn() {
	let rate = FixedU128::from_u32;
	// Moved to a new key.
	ConversionRateToNative::<Test>::insert(v3_asset(1000, 1), rate(1));
	// Collides with an entry already at the new key, which is kept.
	ConversionRateToNative::<Test>::insert(v3_asset(1000, 2), rate(2));
	ConversionRateToNative::<Test>::insert(latest_asset(1000, 2), rate(20));
	// Can't be upgraded, so it's left as is.
	ConversionRateToNative::<Test>::insert(abstract_asset(), rate(3));
	// Already up to date.
	ConversionRateToNative::<Test>::insert(latest_asset(1000, 4), rate(4));

	Spends::<Test>::insert(0, spend(v3_asset(1000, 1), v3_beneficiary(1)));
	// Only the beneficiary needs upgrading.
	Spends::<Test>::insert(1, spend(latest_asset(1000, 1), v3_beneficiary(2)));
	// The asset kind can't be upgraded, so the whole spend is left as is.
	Spends::<Test>::insert(2, spend(abstract_asset(), v3_beneficiary(3)));
	// Already up to date.
	Spends::<Test>::insert(3, spend(latest_asset(1000, 4), latest_beneficiary(4)));

	Bounties::<Test>::insert(0, bounty(v3_asset(1000, 1)));
	Bounties::<Test>::insert(1, bounty(abstract_asset()));
	Bounties::<Test>::insert(2, bounty(latest_asset(1000, 2)));

	move || {
		let mut rates: Vec<_> = ConversionRateToNative::<Test>::iter().collect();
		rates.sort_by_key(|(asset_kind, _)| asset_kind.encode());
		let mut expected = vec![
			(latest_asset(1000, 1), rate(1)),
			(latest_asset(1000, 2), rate(20)),
			(abstract_asset(), rate(3)),
			(latest_asset(1000, 4), rate(4)),
		];
		expected.sort_by_key(|(asset_kind, _)| asset_kind.encode());
		assert_eq!(rates, expected);

		assert_eq!(
			Spends::<Test>::get(0),
			Some(spend(latest_asset(1000, 1), latest_beneficiary(1)))
		);
		assert_eq!(
			Spends::<Test>::get(1),
			Some(spend(latest_asset(1000, 1), latest_beneficiary(2)))
		);
		assert_eq!(Spends::<Test>::get(2), Some(spend(abstract_asset(), v3_beneficiary(3))));
		assert_eq!(
			Spends::<Test>::get(3),
			Some(spend(latest_asset(1000, 4), latest_beneficiary(4)))
		);
		assert_eq!(Spends::<Test>::iter().count(), 4);

		assert_eq!(Bounties::<Test>::get(0), Some(bounty(latest_asset(1000, 1))));
		assert_eq!(Bounties::<Test>::get(1), Some(bounty(abstract_asset())));
		assert_eq!(Bounties::<Test>::get(2), Some(bounty(latest_asset(1000, 2))));
		assert_eq!(Bounties::<Test>::iter().count(), 3);
	}
}

/// Runs the migration with `meter_limit` per step. Returns the number of steps taken.
fn run_stepped(meter_limit: Weight) -> u32 {
	let mut cursor = None;
	let mut steps = 0;
	loop {
		let mut meter = WeightMeter::with_limit(meter_limit);
		cursor = Migration::step(cursor, &mut meter).unwrap();
		steps += 1;
		if cursor.is_none() {
			return steps;
		}
		assert!(steps < 100, "migration doesn't terminate");
	}
}

/// Runs the migration to completion with a single, unlimited meter. Returns the weight consumed.
fn run_all() -> Weight {
	let mut meter = WeightMeter::new();
	let mut cursor = None;
	loop {
		cursor = Migration::step(cursor, &mut meter).unwrap();
		if cursor.is_none() {
			return meter.consumed();
		}
	}
}

#[test]
fn upgrades_all_stages() {
	new_test_ext().execute_with(|| {
		let check = setup();
		let weight = run_all();
		check();
		// At least the base weight once per stage plus one item for each of the 12 entries.
		assert!(weight.all_gte(Weight::from_parts(3 * 10 + 12 * 100, 3 * 10 + 12 * 100)));

		// Running it again changes nothing.
		run_all();
		check();
	});
}

#[test]
fn stepped_upgrades_one_entry_per_step() {
	new_test_ext().execute_with(|| {
		let check = setup();
		// Fits the base weight and a single entry.
		let steps = run_stepped(Weight::from_parts(110, 110));
		check();
		// At least one step per entry, plus one to end each stage and one to end the migration.
		let minimum_steps = 12 + 3 + 1;
		assert!(steps >= minimum_steps, "only {steps} steps");
	});
}

#[test]
fn step_size_does_not_change_the_result() {
	let state_root = || sp_io::storage::root(sp_runtime::StateVersion::V1);
	let small_steps = new_test_ext().execute_with(|| {
		let _ = setup();
		run_stepped(Weight::from_parts(250, 250));
		state_root()
	});
	let one_go = new_test_ext().execute_with(|| {
		let _ = setup();
		run_all();
		state_root()
	});
	assert_eq!(small_steps, one_go);
}

#[test]
fn step_needs_room_for_one_entry() {
	new_test_ext().execute_with(|| {
		let _ = setup();
		let mut meter = WeightMeter::with_limit(Weight::from_parts(109, 109));
		assert_eq!(
			Migration::step(None, &mut meter),
			Err(SteppedMigrationError::InsufficientWeight {
				required: Weight::from_parts(110, 110)
			})
		);
		assert_eq!(meter.consumed(), Weight::zero());
	});
}

#[test]
fn empty_storage_finishes_right_away() {
	new_test_ext().execute_with(|| {
		assert_eq!(run_stepped(Weight::from_parts(110, 110)), 4);
	});
}

#[test]
fn identity_leaves_values_as_they_are() {
	assert_eq!(
		<sp_runtime::traits::Identity as UpgradeToLatest<_>>::upgrade(&v3_beneficiary(1)),
		Ok(None)
	);
}

#[test]
fn converters_upgrade_v3_values() {
	assert_eq!(
		VersionedLocatableAssetToLatest::upgrade(&v3_asset(1000, 1)),
		Ok(Some(latest_asset(1000, 1)))
	);
	assert_eq!(VersionedLocatableAssetToLatest::upgrade(&latest_asset(1000, 1)), Ok(None));
	assert_eq!(VersionedLocatableAssetToLatest::upgrade(&abstract_asset()), Err(()));

	assert_eq!(
		VersionedLocationToLatest::upgrade(&v3_beneficiary(1)),
		Ok(Some(latest_beneficiary(1)))
	);
	assert_eq!(VersionedLocationToLatest::upgrade(&latest_beneficiary(1)), Ok(None));
}

#[test]
fn worst_case_v3_location_can_be_upgraded() {
	let location = VersionedLocation::V3(worst_case_v3_location());
	assert!(matches!(VersionedLocationToLatest::upgrade(&location), Ok(Some(_))));
}

#[test]
fn raw_keys_fit_the_cursor() {
	// The largest key migrated: `ConversionRateToNative`, hashed with `Blake2_128Concat`.
	let max_key_len = 32 +
		<frame_support::Blake2_128Concat as StorageHasher>::max_len::<VersionedLocatableAsset>();
	assert!(max_key_len <= MAX_RAW_KEY_LEN as usize, "{max_key_len} > {MAX_RAW_KEY_LEN}");
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn worst_case_entries_are_upgraded() {
	new_test_ext().execute_with(|| {
		AssetRateMigration::setup_worst_case_entry();
		TreasuryMigration::setup_worst_case_entry();
		BountiesMigration::setup_worst_case_entry();

		let before = (
			ConversionRateToNative::<Test>::iter_keys().next().unwrap(),
			Spends::<Test>::get(0).unwrap(),
			Bounties::<Test>::get(0).unwrap(),
		);
		run_all();

		let rate_key = ConversionRateToNative::<Test>::iter_keys().next().unwrap();
		assert_ne!(rate_key.encode(), before.0.encode());
		let spend = Spends::<Test>::get(0).unwrap();
		assert_ne!(spend.asset_kind.encode(), before.1.asset_kind.encode());
		assert_ne!(spend.beneficiary.encode(), before.1.beneficiary.encode());
		assert_eq!(spend.amount, Balance::max_value());
		let bounty = Bounties::<Test>::get(0).unwrap();
		assert_ne!(bounty.asset_kind.encode(), before.2.asset_kind.encode());
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn try_runtime_checks_pass() {
	new_test_ext().execute_with(|| {
		let _ = setup();
		let state = Migration::pre_upgrade().unwrap();
		run_all();
		Migration::post_upgrade(state).unwrap();
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn try_runtime_post_upgrade_fails_on_leftovers() {
	new_test_ext().execute_with(|| {
		let _ = setup();
		let state = Migration::pre_upgrade().unwrap();
		assert!(Migration::post_upgrade(state).is_err());
	});
}
