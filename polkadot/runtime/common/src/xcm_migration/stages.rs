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

//! The [`MigrationStage`]s of [`super::MigrateXcmToLatest`].

#[cfg(feature = "try-runtime")]
use super::{check_upgraded, count_upgradable, is_latest};
#[cfg(feature = "runtime-benchmarks")]
use super::{old_key, worst_case_stored};
use super::{
	step_over_map, MigrationStage, RawKey, UpgradeToLatest, Upgraded, WeightInfo, LOG_TARGET,
};
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::{
	migrations::SteppedMigrationError, storage::unhashed, traits::tokens::Pay, weights::WeightMeter,
};
use sp_runtime::FixedU128;

/// `pallet_treasury::SpendStatus` as stored before the upgrade.
type OldSpendStatus<T, I, AssetKindUpgrade, BeneficiaryUpgrade> = pallet_treasury::SpendStatus<
	<AssetKindUpgrade as UpgradeToLatest<<T as pallet_treasury::Config<I>>::AssetKind>>::Stored,
	pallet_treasury::AssetBalanceOf<T, I>,
	<BeneficiaryUpgrade as UpgradeToLatest<<T as pallet_treasury::Config<I>>::Beneficiary>>::Stored,
	pallet_treasury::BlockNumberFor<T, I>,
	<<T as pallet_treasury::Config<I>>::Paymaster as Pay>::Id,
>;

/// `pallet_multi_asset_bounties::Bounty` as stored before the upgrade.
type OldBounty<T, I, AssetKindUpgrade> = pallet_multi_asset_bounties::Bounty<
	<T as frame_system::Config>::AccountId,
	<T as pallet_multi_asset_bounties::Config<I>>::Balance,
	<AssetKindUpgrade as UpgradeToLatest<
		<T as pallet_multi_asset_bounties::Config<I>>::AssetKind,
	>>::Stored,
	<T as frame_system::Config>::Hash,
	pallet_multi_asset_bounties::PaymentIdOf<T, I>,
	<T as pallet_multi_asset_bounties::Config<I>>::Beneficiary,
>;

/// Re-keys `pallet_asset_rate::ConversionRateToNative`, upgrading every asset kind with
/// `Upgrade`.
///
/// The map is keyed by the asset kind, so upgrading one moves its conversion rate to a new key.
/// If a conversion rate already exists at the new key, that one is kept and the old one dropped.
/// Conversion rates of asset kinds that can't be upgraded are removed.
pub struct AssetRateStage<T, Upgrade, W>(PhantomData<(T, Upgrade, W)>);

impl<T, Upgrade, W> MigrationStage for AssetRateStage<T, Upgrade, W>
where
	T: pallet_asset_rate::Config,
	Upgrade: UpgradeToLatest<T::AssetKind>,
	W: WeightInfo,
{
	fn step(
		last_key: Option<RawKey>,
		meter: &mut WeightMeter,
	) -> Result<Option<RawKey>, SteppedMigrationError> {
		use pallet_asset_rate::ConversionRateToNative as Rates;

		step_over_map::<Rates<T>, _, _, Upgrade::Stored, FixedU128, W>(
			last_key,
			meter,
			|raw_key, old_asset_kind, rate| match Upgrade::upgrade(old_asset_kind) {
				Ok(Upgraded::Unchanged(_)) => {},
				Ok(Upgraded::Changed(new_asset_kind)) => {
					unhashed::kill(raw_key);
					let new_key = Rates::<T>::hashed_key_for(&new_asset_kind);
					if unhashed::exists(&new_key) {
						log::warn!(
							target: LOG_TARGET,
							"Conversion rate for {:?} already exists, dropping the old one",
							new_asset_kind,
						);
					} else {
						unhashed::put(&new_key, &rate);
					}
				},
				Err(old_asset_kind) => {
					log::error!(
						target: LOG_TARGET,
						"Cannot upgrade asset kind {:?}, removing its conversion rate",
						old_asset_kind,
					);
					unhashed::kill(raw_key);
				},
			},
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_worst_case_entry() {
		use pallet_asset_rate::{AssetKindFactory, ConversionRateToNative as Rates};

		let asset_kind = worst_case_stored::<Upgrade, _>(|| {
			<T as pallet_asset_rate::Config>::BenchmarkHelper::create_asset_kind(0)
		});
		unhashed::put(&old_key::<Rates<T>, _, _>(&asset_kind), &FixedU128::from_u32(1));
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use pallet_asset_rate::ConversionRateToNative as Rates;

		Ok(count_upgradable::<Rates<T>, _, _, Upgrade::Stored, FixedU128>(|asset_kind, _| {
			Upgrade::upgrade(asset_kind.clone()).is_ok()
		}))
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (upgradable, after) =
			check_upgraded::<pallet_asset_rate::ConversionRateToNative<T>, _, _>(
				state,
				"ConversionRateToNative",
				|asset_kind, _| is_latest::<Upgrade, _>(asset_kind),
			)?;
		// Entries can only be dropped when two asset kinds upgrade to the same one.
		frame_support::ensure!(after <= upgradable, "ConversionRateToNative gained entries");
		Ok(())
	}
}

/// Upgrades the asset kind and beneficiary of every spend in `pallet_treasury::Spends`, with
/// `AssetKindUpgrade` and `BeneficiaryUpgrade`.
///
/// Use [`sp_runtime::traits::Identity`] for a part that holds no outdated XCM data. A spend is
/// removed if either part can't be upgraded.
pub struct TreasurySpendsStage<T, I, AssetKindUpgrade, BeneficiaryUpgrade, W>(
	PhantomData<(T, I, AssetKindUpgrade, BeneficiaryUpgrade, W)>,
);

impl<T, I, AssetKindUpgrade, BeneficiaryUpgrade, W> MigrationStage
	for TreasurySpendsStage<T, I, AssetKindUpgrade, BeneficiaryUpgrade, W>
where
	T: pallet_treasury::Config<I>,
	I: 'static,
	AssetKindUpgrade: UpgradeToLatest<T::AssetKind>,
	BeneficiaryUpgrade: UpgradeToLatest<T::Beneficiary>,
	W: WeightInfo,
{
	fn step(
		last_key: Option<RawKey>,
		meter: &mut WeightMeter,
	) -> Result<Option<RawKey>, SteppedMigrationError> {
		use pallet_treasury::{SpendIndex, SpendStatus, Spends};

		step_over_map::<
			Spends<T, I>,
			_,
			_,
			SpendIndex,
			OldSpendStatus<T, I, AssetKindUpgrade, BeneficiaryUpgrade>,
			W,
		>(last_key, meter, |_, index, spend| {
			let SpendStatus { asset_kind, amount, beneficiary, valid_from, expire_at, status } =
				spend;
			match (AssetKindUpgrade::upgrade(asset_kind), BeneficiaryUpgrade::upgrade(beneficiary))
			{
				(Ok(asset_kind), Ok(beneficiary)) => {
					if asset_kind.is_changed() || beneficiary.is_changed() {
						Spends::<T, I>::insert(
							index,
							SpendStatus {
								asset_kind: asset_kind.into_inner(),
								amount,
								beneficiary: beneficiary.into_inner(),
								valid_from,
								expire_at,
								status,
							},
						);
					}
				},
				(asset_kind, beneficiary) => {
					log::error!(
						target: LOG_TARGET,
						"Cannot upgrade treasury spend {}, removing it. Asset kind: {:?}, \
						beneficiary: {:?}",
						index,
						asset_kind,
						beneficiary,
					);
					Spends::<T, I>::remove(index);
				},
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_worst_case_entry() {
		use pallet_treasury::ArgumentsFactory;
		use sp_runtime::traits::Bounded;
		type Helper<T, I> = <T as pallet_treasury::Config<I>>::BenchmarkHelper;

		let spend: OldSpendStatus<T, I, AssetKindUpgrade, BeneficiaryUpgrade> =
			pallet_treasury::SpendStatus {
				asset_kind: worst_case_stored::<AssetKindUpgrade, _>(|| {
					Helper::<T, I>::create_asset_kind(0)
				}),
				amount: Bounded::max_value(),
				beneficiary: worst_case_stored::<BeneficiaryUpgrade, _>(|| {
					Helper::<T, I>::create_beneficiary([0; 32])
				}),
				valid_from: Bounded::max_value(),
				expire_at: Bounded::max_value(),
				status: pallet_treasury::PaymentState::Pending,
			};
		unhashed::put(&pallet_treasury::Spends::<T, I>::hashed_key_for(0), &spend);
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use pallet_treasury::{SpendIndex, Spends};

		Ok(count_upgradable::<
			Spends<T, I>,
			_,
			_,
			SpendIndex,
			OldSpendStatus<T, I, AssetKindUpgrade, BeneficiaryUpgrade>,
		>(|_, spend| {
			AssetKindUpgrade::upgrade(spend.asset_kind.clone()).is_ok() &&
				BeneficiaryUpgrade::upgrade(spend.beneficiary.clone()).is_ok()
		}))
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (upgradable, after) =
			check_upgraded::<pallet_treasury::Spends<T, I>, _, _>(state, "Spends", |_, spend| {
				is_latest::<AssetKindUpgrade, _>(&spend.asset_kind) &&
					is_latest::<BeneficiaryUpgrade, _>(&spend.beneficiary)
			})?;
		frame_support::ensure!(after == upgradable, "Spends lost entries that could be upgraded");
		Ok(())
	}
}

/// Upgrades the asset kind of every bounty in `pallet_multi_asset_bounties::Bounties` with
/// `AssetKindUpgrade`.
///
/// Child bounties use the asset kind of their parent and don't store one themselves. Beneficiaries
/// are left as they are: no runtime uses a beneficiary type that holds XCM v3 data.
///
/// A bounty whose asset kind can't be upgraded is removed from `Bounties`. Nothing else is
/// touched: its child bounties and any funds held by its account are no longer tracked by it.
pub struct BountiesStage<T, I, AssetKindUpgrade, W>(PhantomData<(T, I, AssetKindUpgrade, W)>);

impl<T, I, AssetKindUpgrade, W> MigrationStage for BountiesStage<T, I, AssetKindUpgrade, W>
where
	T: pallet_multi_asset_bounties::Config<I>,
	I: 'static,
	AssetKindUpgrade: UpgradeToLatest<T::AssetKind>,
	W: WeightInfo,
{
	fn step(
		last_key: Option<RawKey>,
		meter: &mut WeightMeter,
	) -> Result<Option<RawKey>, SteppedMigrationError> {
		use pallet_multi_asset_bounties::{Bounties, Bounty, BountyIndex};

		step_over_map::<Bounties<T, I>, _, _, BountyIndex, OldBounty<T, I, AssetKindUpgrade>, W>(
			last_key,
			meter,
			|_, index, bounty| {
				let Bounty { asset_kind, value, metadata, status } = bounty;
				match AssetKindUpgrade::upgrade(asset_kind) {
					Ok(Upgraded::Unchanged(_)) => {},
					Ok(Upgraded::Changed(asset_kind)) => Bounties::<T, I>::insert(
						index,
						Bounty { asset_kind, value, metadata, status },
					),
					Err(asset_kind) => {
						log::error!(
							target: LOG_TARGET,
							"Cannot upgrade the asset kind of bounty {}, removing it: {:?}",
							index,
							asset_kind,
						);
						Bounties::<T, I>::remove(index);
					},
				}
			},
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_worst_case_entry() {
		use pallet_multi_asset_bounties::{ArgumentsFactory, Bounty, BountyStatus, PaymentState};
		use sp_runtime::traits::Bounded;
		type Helper<T, I> = <T as pallet_multi_asset_bounties::Config<I>>::BenchmarkHelper;

		let bounty: OldBounty<T, I, AssetKindUpgrade> = Bounty {
			asset_kind: worst_case_stored::<AssetKindUpgrade, _>(|| {
				Helper::<T, I>::create_asset_kind(0)
			}),
			value: Bounded::max_value(),
			metadata: Default::default(),
			// The largest status.
			status: BountyStatus::PayoutAttempted {
				curator: frame_benchmarking::account("curator", 0, 0),
				beneficiary: Helper::<T, I>::create_beneficiary([0; 32]),
				payment_status: PaymentState::Pending,
			},
		};
		unhashed::put(&pallet_multi_asset_bounties::Bounties::<T, I>::hashed_key_for(0), &bounty);
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use pallet_multi_asset_bounties::{Bounties, BountyIndex};

		Ok(
			count_upgradable::<Bounties<T, I>, _, _, BountyIndex, OldBounty<T, I, AssetKindUpgrade>>(
				|_, bounty| AssetKindUpgrade::upgrade(bounty.asset_kind.clone()).is_ok(),
			),
		)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (upgradable, after) =
			check_upgraded::<pallet_multi_asset_bounties::Bounties<T, I>, _, _>(
				state,
				"Bounties",
				|_, bounty| is_latest::<AssetKindUpgrade, _>(&bounty.asset_kind),
			)?;
		frame_support::ensure!(after == upgradable, "Bounties lost entries that could be upgraded");
		Ok(())
	}
}
