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
use super::{check_upgraded, count_entries};
use super::{step_over_map, MigrationStage, RawKey, UpgradeToLatest, WeightInfo, LOG_TARGET};
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{migrations::SteppedMigrationError, weights::WeightMeter};

/// Re-keys `pallet_asset_rate::ConversionRateToNative`, upgrading every asset kind with
/// `Upgrade`.
///
/// The map is keyed by the asset kind, so upgrading one moves its conversion rate to a new key.
/// If a conversion rate already exists at the new key, that one is kept and the old one dropped.
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

		step_over_map::<Rates<T>, _, _, W>(last_key, meter, |old_asset_kind, rate| {
			let new_asset_kind = match Upgrade::upgrade(&old_asset_kind) {
				Ok(Some(new_asset_kind)) => new_asset_kind,
				Ok(None) => return,
				Err(()) => {
					log::error!(
						target: LOG_TARGET,
						"Cannot upgrade asset kind {:?}, leaving its conversion rate untouched",
						old_asset_kind,
					);
					return;
				},
			};

			Rates::<T>::remove(&old_asset_kind);
			if Rates::<T>::contains_key(&new_asset_kind) {
				log::warn!(
					target: LOG_TARGET,
					"Conversion rate for {:?} already exists, dropping the one of {:?}",
					new_asset_kind,
					old_asset_kind,
				);
			} else {
				Rates::<T>::insert(&new_asset_kind, rate);
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_worst_case_entry() {
		use pallet_asset_rate::AssetKindFactory;

		let asset_kind = Upgrade::worst_case_outdated().unwrap_or_else(|| {
			<T as pallet_asset_rate::Config>::BenchmarkHelper::create_asset_kind(0)
		});
		pallet_asset_rate::ConversionRateToNative::<T>::insert(
			asset_kind,
			sp_runtime::FixedU128::from_u32(1),
		);
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(count_entries::<pallet_asset_rate::ConversionRateToNative<T>, _>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (before, after) = check_upgraded::<pallet_asset_rate::ConversionRateToNative<T>, _, _>(
			state,
			"ConversionRateToNative",
			|asset_kind, _| outdated::<Upgrade, _>(asset_kind),
		)?;
		// Entries can only be dropped when two asset kinds upgrade to the same one.
		frame_support::ensure!(after <= before, "ConversionRateToNative gained entries");
		Ok(())
	}
}

/// Upgrades the asset kind and beneficiary of every spend in `pallet_treasury::Spends`, with
/// `AssetKindUpgrade` and `BeneficiaryUpgrade`.
///
/// Use [`sp_runtime::traits::Identity`] for a part that holds no outdated XCM data. A spend is
/// left untouched if either part can't be upgraded.
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
		use pallet_treasury::Spends;

		step_over_map::<Spends<T, I>, _, _, W>(last_key, meter, |index, spend| {
			match (
				AssetKindUpgrade::upgrade(&spend.asset_kind),
				BeneficiaryUpgrade::upgrade(&spend.beneficiary),
			) {
				(Ok(None), Ok(None)) => {},
				(Ok(asset_kind), Ok(beneficiary)) => Spends::<T, I>::insert(
					index,
					pallet_treasury::SpendStatus {
						asset_kind: asset_kind.unwrap_or(spend.asset_kind),
						beneficiary: beneficiary.unwrap_or(spend.beneficiary),
						..spend
					},
				),
				_ => log::error!(
					target: LOG_TARGET,
					"Cannot upgrade treasury spend {}, leaving it untouched: {:?}",
					index,
					spend,
				),
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_worst_case_entry() {
		use pallet_treasury::ArgumentsFactory;
		use sp_runtime::traits::Bounded;
		type Helper<T, I> = <T as pallet_treasury::Config<I>>::BenchmarkHelper;

		let spend = pallet_treasury::SpendStatus {
			asset_kind: AssetKindUpgrade::worst_case_outdated()
				.unwrap_or_else(|| Helper::<T, I>::create_asset_kind(0)),
			amount: Bounded::max_value(),
			beneficiary: BeneficiaryUpgrade::worst_case_outdated()
				.unwrap_or_else(|| Helper::<T, I>::create_beneficiary([0; 32])),
			valid_from: Bounded::max_value(),
			expire_at: Bounded::max_value(),
			status: pallet_treasury::PaymentState::Pending,
		};
		pallet_treasury::Spends::<T, I>::insert(0, spend);
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(count_entries::<pallet_treasury::Spends<T, I>, _>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (before, after) =
			check_upgraded::<pallet_treasury::Spends<T, I>, _, _>(state, "Spends", |_, spend| {
				// A spend is only upgraded if both parts can be.
				let asset_kind = outdated::<AssetKindUpgrade, _>(&spend.asset_kind)?;
				let beneficiary = outdated::<BeneficiaryUpgrade, _>(&spend.beneficiary)?;
				Some(asset_kind || beneficiary)
			})?;
		frame_support::ensure!(after == before, "Spends changed its number of entries");
		Ok(())
	}
}

/// Upgrades the asset kind of every bounty in `pallet_multi_asset_bounties::Bounties` with
/// `AssetKindUpgrade`.
///
/// Child bounties use the asset kind of their parent and don't store one themselves. Beneficiaries
/// are left as they are: no runtime uses a beneficiary type that holds XCM v3 data.
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
		use pallet_multi_asset_bounties::Bounties;

		step_over_map::<Bounties<T, I>, _, _, W>(last_key, meter, |index, bounty| {
			match AssetKindUpgrade::upgrade(&bounty.asset_kind) {
				Ok(Some(asset_kind)) => Bounties::<T, I>::insert(
					index,
					pallet_multi_asset_bounties::Bounty { asset_kind, ..bounty },
				),
				Ok(None) => {},
				Err(()) => log::error!(
					target: LOG_TARGET,
					"Cannot upgrade the asset kind of bounty {}, leaving it untouched: {:?}",
					index,
					bounty.asset_kind,
				),
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_worst_case_entry() {
		use pallet_multi_asset_bounties::{ArgumentsFactory, Bounty, BountyStatus, PaymentState};
		use sp_runtime::traits::Bounded;
		type Helper<T, I> = <T as pallet_multi_asset_bounties::Config<I>>::BenchmarkHelper;

		let bounty = Bounty {
			asset_kind: AssetKindUpgrade::worst_case_outdated()
				.unwrap_or_else(|| Helper::<T, I>::create_asset_kind(0)),
			value: Bounded::max_value(),
			metadata: Default::default(),
			// The largest status.
			status: BountyStatus::PayoutAttempted {
				curator: frame_benchmarking::account("curator", 0, 0),
				beneficiary: Helper::<T, I>::create_beneficiary([0; 32]),
				payment_status: PaymentState::Pending,
			},
		};
		pallet_multi_asset_bounties::Bounties::<T, I>::insert(0, bounty);
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(count_entries::<pallet_multi_asset_bounties::Bounties<T, I>, _>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (before, after) = check_upgraded::<pallet_multi_asset_bounties::Bounties<T, I>, _, _>(
			state,
			"Bounties",
			|_, bounty| outdated::<AssetKindUpgrade, _>(&bounty.asset_kind),
		)?;
		frame_support::ensure!(after == before, "Bounties changed its number of entries");
		Ok(())
	}
}

/// Whether `value` still needs upgrading, or `None` if it can't be upgraded.
#[cfg(feature = "try-runtime")]
fn outdated<Upgrade: UpgradeToLatest<T>, T>(value: &T) -> Option<bool> {
	Upgrade::upgrade(value).ok().map(|upgraded| upgraded.is_some())
}
