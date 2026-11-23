// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Converts reported units of a validator service into era points, intended to be used in Asset
//! Hub.
//!
//! Receives the blocks validators author on system chains and on the chain that runs this pallet.
//!
//! ## Overview
//!
//! Each system chain counts the blocks its authors produce in `pallet-validator-collators` and
//! reports the counts of the validators. Other chains call [`Pallet::report_authored_blocks`]
//! from [`Config::ReportOrigin`], which names the reporting chain as a [`Config::Source`]. The
//! chain that runs this pallet credits its own counts through [`Pallet::credit_authored_blocks`].
//! A credited count becomes `count × PointsPerService[(source, Collation)]` era points, handed to
//! [`Config::RewardPoints`] as [`sp_staking::ServiceKind::Collation`] points. A source without an
//! entry for [`sp_staking::ServiceKind::Collation`] in [`PointsPerService`] earns nothing.
//! Governance sets the entries through [`Config::PointsOrigin`]. Relay chain consensus points are
//! not converted here.
//!
//! For example, with `PointsPerService[(source, Collation)] = 10` for a reporting chain, a report
//! from that chain of 40 units (blocks authored, for
//! [`sp_staking::ServiceKind::Collation`]) for an account credits it 400 era points.
//!
//! ## Limits
//!
//! Each count is first clamped to [`Config::MaxBlocksPerEntry`]. A source is then credited at
//! most [`UnitsPerEraLimit`] units of a service per era, and [`CreditedUnits`] tracks what it was
//! credited in the active era. When a report asks for more than what is left, every entry is cut
//! by the same ratio, rounded down. Units that are not credited are dropped and reported in
//! [`Event::UnitsOverLimit`]. Governance sets the limits through [`Config::PointsOrigin`].
//!
//! For example, with 80 units left in the era, a report of 50 blocks for each of three accounts
//! asks for 150. Each account is credited `50 × 80 / 150 = 26` blocks and 72 blocks are over the
//! limit.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	pub use crate::weights::WeightInfo;
	use alloc::vec::Vec;
	use frame_support::{pallet_prelude::*, DefaultNoBound};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{Convert, MaybeSerializeDeserialize, SaturatedConversion, Saturating};
	use sp_staking::{EraIndex, RewardPoints, ServiceKind};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Identifies a chain whose blocks earn era points.
		type Source: Parameter + MaxEncodedLen + Ord + Copy + MaybeSerializeDeserialize;

		/// Origin of a chain allowed to report the blocks authored on it. Returns that chain.
		type ReportOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::Source>;

		/// Origin allowed to set the points per service of a source.
		type PointsOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Credits era points to the validators.
		type RewardPoints: RewardPoints<Self::AccountId>;

		/// Largest points per unit of a service [`Pallet::set_points_per_service`] and the genesis
		/// config accept, per service.
		///
		/// Crediting also bounds a stored value by it.
		type MaxPointsPerService: Convert<ServiceKind, u32>;

		/// Largest count of blocks credited for one entry of a report. A larger count is credited
		/// as this many blocks.
		///
		/// Each count is clamped before the limit in [`UnitsPerEraLimit`] applies. The clamp has no
		/// effect for a source whose limit is not above it.
		#[pallet::constant]
		type MaxBlocksPerEntry: Get<u32>;

		/// Units of a service credited to a source per era until governance sets
		/// [`UnitsPerEraLimit`] for that source and service, per service.
		type DefaultUnitsPerEraLimit: Convert<ServiceKind, u32>;

		/// Largest limit of a service [`Pallet::set_units_per_era_limit`] accepts, per service.
		///
		/// Crediting also bounds a stored limit by it.
		type MaxUnitsPerEraLimit: Convert<ServiceKind, u32>;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Era points a validator earns for each unit of a service on a source, bounded by
	/// [`Config::MaxPointsPerService`] when crediting. Absent means zero.
	///
	/// A unit is specific to the service. For [`ServiceKind::Collation`] it is one block
	/// authored. The result is credited to staking as era points.
	#[pallet::storage]
	pub type PointsPerService<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::Source, Twox64Concat, ServiceKind, u32, ValueQuery>;

	/// Units of a service credited to a source per era, bounded by
	/// [`Config::MaxUnitsPerEraLimit`] when crediting. Zero credits nothing. Absent means
	/// [`Config::DefaultUnitsPerEraLimit`] of the service.
	///
	/// A unit is the one of [`PointsPerService`].
	#[pallet::storage]
	pub type UnitsPerEraLimit<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::Source, Twox64Concat, ServiceKind, u32, OptionQuery>;

	/// The era a source was last credited in for a service, and the units credited to it in
	/// that era. The units count as zero in any other era.
	#[pallet::storage]
	pub type CreditedUnits<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::Source,
		Twox64Concat,
		ServiceKind,
		(EraIndex, u32),
		ValueQuery,
	>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial points per service, per source.
		pub points_per_service: Vec<(T::Source, ServiceKind, u32)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (source, service, points) in
				self.points_per_service.iter().filter(|(_, _, points)| *points > 0)
			{
				assert!(
					*service != ServiceKind::RelayConsensus,
					"the genesis config weights points that come from the relay chain"
				);
				assert!(
					*points <= T::MaxPointsPerService::convert(*service),
					"points per service in the genesis config exceed `MaxPointsPerService`"
				);
				PointsPerService::<T>::insert(source, service, points);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Validators earn `points` era points for each unit of `service` on `source`.
		PointsPerServiceSet { source: T::Source, service: ServiceKind, points: u32 },
		/// Era points for `units` units of `service` on `source` were handed to
		/// [`Config::RewardPoints`].
		UnitsCredited { source: T::Source, service: ServiceKind, units: u32 },
		/// `units` units of `service` on `source` were not credited, because `source` earns no
		/// points for `service` or no era is active.
		UnitsIgnored { source: T::Source, service: ServiceKind, units: u32 },
		/// `source` is credited at most `limit` units of `service` per era.
		UnitsPerEraLimitSet { source: T::Source, service: ServiceKind, limit: u32 },
		/// `units` units of `service` on `source` were not credited in `era`, because they exceed
		/// [`Config::MaxBlocksPerEntry`] or the limit of `source` for `service` in
		/// [`UnitsPerEraLimit`].
		UnitsOverLimit { source: T::Source, service: ServiceKind, era: EraIndex, units: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The points per service exceed [`Config::MaxPointsPerService`].
		PointsPerServiceTooHigh,
		/// Relay consensus points are weighted by the relay chain, not by this pallet.
		///
		/// Note: This is because relay-chain issues its own points, and never sends its units of
		/// work to this pallet to be converted to points. This is for legacy reasons, and while
		/// unifying the interface would be more ergonomic, we keep it as-is for now due to JAM
		/// being on the horizon, and the effort of altering relay chain not being considered
		/// worthwhile.
		RelayConsensusNotWeighted,
		/// The limit exceeds [`Config::MaxUnitsPerEraLimit`].
		UnitsPerEraLimitTooHigh,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			// The only service this pallet credits.
			let service = ServiceKind::Collation;
			let max_points = T::MaxPointsPerService::convert(service);
			let default_limit = T::DefaultUnitsPerEraLimit::convert(service);
			let max_limit = T::MaxUnitsPerEraLimit::convert(service);
			assert!(max_points > 0, "`MaxPointsPerService` must not be zero");
			assert!(default_limit > 0, "`DefaultUnitsPerEraLimit` must not be zero");
			assert!(
				default_limit <= max_limit,
				"`DefaultUnitsPerEraLimit` must not exceed `MaxUnitsPerEraLimit`"
			);
			assert!(
				max_limit.checked_mul(max_points).is_some(),
				"`MaxUnitsPerEraLimit` times `MaxPointsPerService` must fit a `u32`"
			);
			assert!(T::MaxBlocksPerEntry::get() > 0, "`MaxBlocksPerEntry` must not be zero");
			assert!(
				T::MaxBlocksPerEntry::get() <= default_limit,
				"`MaxBlocksPerEntry` must not exceed `DefaultUnitsPerEraLimit`"
			);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the era points a validator earns for each unit of `service` on `source`.
		///
		/// Zero removes the entry, so `source` earns nothing for `service`. Fails with
		/// [`Error::PointsPerServiceTooHigh`] above [`Config::MaxPointsPerService`], and with
		/// [`Error::RelayConsensusNotWeighted`] for [`ServiceKind::RelayConsensus`].
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_points_per_service())]
		pub fn set_points_per_service(
			origin: OriginFor<T>,
			source: T::Source,
			service: ServiceKind,
			points: u32,
		) -> DispatchResult {
			T::PointsOrigin::ensure_origin(origin)?;
			ensure!(service != ServiceKind::RelayConsensus, Error::<T>::RelayConsensusNotWeighted);
			ensure!(
				points <= T::MaxPointsPerService::convert(service),
				Error::<T>::PointsPerServiceTooHigh
			);
			match points {
				0 => PointsPerService::<T>::remove(source, service),
				points => PointsPerService::<T>::insert(source, service, points),
			}
			Self::deposit_event(Event::PointsPerServiceSet { source, service, points });
			Ok(())
		}

		/// Credit the blocks the validators authored on the calling chain.
		///
		/// The weight of crediting is refunded for the entries not handed to
		/// [`Config::RewardPoints`].
		#[pallet::call_index(1)]
		#[pallet::weight(Pallet::<T>::report_authored_blocks_weight(
			blocks.len() as u32,
			blocks.len() as u32,
		))]
		pub fn report_authored_blocks(
			origin: OriginFor<T>,
			blocks: Vec<(T::AccountId, u32)>,
		) -> DispatchResultWithPostInfo {
			let source = T::ReportOrigin::ensure_origin(origin)?;
			let entries = blocks.len() as u32;
			let credited = Self::credit_authored_blocks(source, blocks);
			Ok(Some(Self::report_authored_blocks_weight(entries, credited)).into())
		}

		/// Set the units of `service` credited to `source` per era.
		///
		/// Zero is stored and stops crediting for `source` and `service` without changing
		/// [`PointsPerService`]. The limit applies from the next report on. Units already credited
		/// in the active era stay credited. Fails with [`Error::UnitsPerEraLimitTooHigh`] above
		/// [`Config::MaxUnitsPerEraLimit`], and with [`Error::RelayConsensusNotWeighted`] for
		/// [`ServiceKind::RelayConsensus`].
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_units_per_era_limit())]
		pub fn set_units_per_era_limit(
			origin: OriginFor<T>,
			source: T::Source,
			service: ServiceKind,
			limit: u32,
		) -> DispatchResult {
			T::PointsOrigin::ensure_origin(origin)?;
			ensure!(service != ServiceKind::RelayConsensus, Error::<T>::RelayConsensusNotWeighted);
			ensure!(
				limit <= T::MaxUnitsPerEraLimit::convert(service),
				Error::<T>::UnitsPerEraLimitTooHigh
			);
			UnitsPerEraLimit::<T>::insert(source, service, limit);
			Self::deposit_event(Event::UnitsPerEraLimitSet { source, service, limit });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Credit `blocks` authored on `source` as era points in the active era, with the points
		/// per service of `source` for [`ServiceKind::Collation`].
		///
		/// Each count is clamped to [`Config::MaxBlocksPerEntry`]. If the clamped counts exceed
		/// what is left of the limit of `source` in [`UnitsPerEraLimit`] for the active era, every
		/// count is cut by the same ratio, rounded down. The outcome is one of:
		/// - nothing is credited when `blocks` is empty, with no event.
		/// - nothing is credited when `source` earns no points or no era is active, with
		///   [`Event::UnitsIgnored`]. The limit of `source` is not used.
		/// - otherwise, entries credited at least one block are handed to [`Config::RewardPoints`]
		///   with [`Event::UnitsCredited`], and blocks not credited are reported in
		///   [`Event::UnitsOverLimit`].
		///
		/// Returns the number of entries handed to [`Config::RewardPoints`].
		pub fn credit_authored_blocks(
			source: T::Source,
			blocks: impl IntoIterator<Item = (T::AccountId, u32)>,
		) -> u32 {
			let service = ServiceKind::Collation;
			let max_blocks = T::MaxBlocksPerEntry::get();
			let mut requested = 0u64;
			let blocks: Vec<_> = blocks
				.into_iter()
				.map(|(who, count)| {
					requested.saturating_accrue(count.into());
					(who, count.min(max_blocks))
				})
				.collect();
			if blocks.is_empty() {
				return 0;
			}

			let Some((era, points_per_unit)) = Self::era_and_points_per_unit(source, service)
			else {
				Self::deposit_event(Event::UnitsIgnored {
					source,
					service,
					units: requested.saturated_into(),
				});
				return 0;
			};

			let (used, remaining) = Self::units_used_and_remaining(source, service, era);
			let credits = Self::fit_to_remaining(blocks, remaining);
			let credited =
				credits.iter().fold(0u32, |total, (_, credit)| total.saturating_add(*credit));
			CreditedUnits::<T>::insert(source, service, (era, used.saturating_add(credited)));

			let points: Vec<_> = credits
				.into_iter()
				.filter(|(_, credit)| *credit > 0)
				.map(|(who, credit)| (who, credit.saturating_mul(points_per_unit)))
				.collect();
			let entries = points.len() as u32;
			if !points.is_empty() {
				T::RewardPoints::reward(service, points);
				Self::deposit_event(Event::UnitsCredited { source, service, units: credited });
			}
			let over = requested.saturating_sub(credited.into());
			if over > 0 {
				Self::deposit_event(Event::UnitsOverLimit {
					source,
					service,
					era,
					units: over.saturated_into(),
				});
			}
			entries
		}

		/// The active era and the points per unit of `service` on `source`, or `None` if `source`
		/// earns no points for `service` or no era is active.
		fn era_and_points_per_unit(
			source: T::Source,
			service: ServiceKind,
		) -> Option<(EraIndex, u32)> {
			let points_per_unit = PointsPerService::<T>::get(source, service)
				.min(T::MaxPointsPerService::convert(service));
			if points_per_unit == 0 {
				return None;
			}
			T::RewardPoints::active_era().map(|era| (era, points_per_unit))
		}

		/// The units of `service` already credited to `source` in `era`, and what is left of its
		/// limit.
		fn units_used_and_remaining(
			source: T::Source,
			service: ServiceKind,
			era: EraIndex,
		) -> (u32, u32) {
			let (counter_era, counter_units) = CreditedUnits::<T>::get(source, service);
			let used = if counter_era == era { counter_units } else { 0 };
			let limit = UnitsPerEraLimit::<T>::get(source, service)
				.unwrap_or_else(|| T::DefaultUnitsPerEraLimit::convert(service))
				.min(T::MaxUnitsPerEraLimit::convert(service));
			(used, limit.saturating_sub(used))
		}

		/// `counts` unchanged if their total fits in `remaining`, otherwise every count cut by the
		/// same ratio, rounded down.
		fn fit_to_remaining(
			counts: Vec<(T::AccountId, u32)>,
			remaining: u32,
		) -> Vec<(T::AccountId, u32)> {
			let total = counts
				.iter()
				.fold(0u64, |total, (_, count)| total.saturating_add((*count).into()));
			if total <= remaining.into() {
				return counts;
			}
			// `total > remaining`, so the divisor is not zero and each count only shrinks.
			counts
				.into_iter()
				.map(|(who, count)| {
					let credit = u64::from(count) * u64::from(remaining) / total;
					(who, credit.saturated_into())
				})
				.collect()
		}

		/// Weight of [`Self::report_authored_blocks`] for a report of `entries` entries, of which
		/// `credited` are handed to [`Config::RewardPoints`].
		pub fn report_authored_blocks_weight(entries: u32, credited: u32) -> Weight {
			let reward = match credited {
				0 => Weight::zero(),
				credited => T::RewardPoints::weight(credited),
			};
			T::WeightInfo::report_authored_blocks(entries).saturating_add(reward)
		}
	}
}
