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
//! A count becomes `min(count, MaxBlocksPerEntry) × PointsPerService[(source, Collation)]` era
//! points, handed to [`Config::RewardPoints`] as [`sp_staking::ValidatorServiceKind::Collation`]
//! points. A source without an entry for [`sp_staking::ValidatorServiceKind::Collation`] in
//! [`PointsPerService`] earns nothing. Governance sets the entries through
//! [`Config::PointsOrigin`]. Relay chain consensus points are not converted here.
//!
//! For example, with `PointsPerService[(source, Collation)] = 10` for a reporting chain, a report
//! from that chain of 40 units (blocks authored, for
//! [`sp_staking::ValidatorServiceKind::Collation`]) for an account credits it 400 era points.

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
	use sp_runtime::traits::MaybeSerializeDeserialize;
	use sp_staking::{RewardPoints, ValidatorServiceKind};

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

		/// Largest points per service [`Pallet::set_points_per_service`] and the genesis config
		/// accept, for every service.
		///
		/// Crediting also bounds a stored value by it.
		#[pallet::constant]
		type MaxPointsPerService: Get<u32>;

		/// Largest count of blocks credited for one entry of a report. A larger count is credited
		/// as this many blocks.
		#[pallet::constant]
		type MaxBlocksPerEntry: Get<u32>;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Era points a validator earns for each unit of a service on a source, bounded by
	/// [`Config::MaxPointsPerService`] when crediting. Absent means zero.
	///
	/// A unit is specific to the service. For [`ValidatorServiceKind::Collation`] it is one block
	/// authored. The result is credited to staking as era points.
	#[pallet::storage]
	pub type PointsPerService<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::Source,
		Twox64Concat,
		ValidatorServiceKind,
		u32,
		ValueQuery,
	>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial points per service, per source.
		pub points_per_service: Vec<(T::Source, ValidatorServiceKind, u32)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (source, service, points) in
				self.points_per_service.iter().filter(|(_, _, points)| *points > 0)
			{
				assert!(
					*service != ValidatorServiceKind::RelayConsensus,
					"the genesis config weights points that come from the relay chain"
				);
				assert!(
					*points <= T::MaxPointsPerService::get(),
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
		PointsPerServiceSet { source: T::Source, service: ValidatorServiceKind, points: u32 },
		/// Era points for the blocks of `entries` entries from `source` were handed to
		/// [`Config::RewardPoints`].
		BlocksCredited { source: T::Source, entries: u32 },
		/// The blocks of `entries` entries from `source` earned no points and were not credited.
		BlocksIgnored { source: T::Source, entries: u32 },
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
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(T::MaxPointsPerService::get() > 0, "`MaxPointsPerService` must not be zero");
			assert!(T::MaxBlocksPerEntry::get() > 0, "`MaxBlocksPerEntry` must not be zero");
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the era points a validator earns for each unit of `service` on `source`.
		///
		/// Zero removes the entry, so `source` earns nothing for `service`. Fails with
		/// [`Error::PointsPerServiceTooHigh`] above [`Config::MaxPointsPerService`], and with
		/// [`Error::RelayConsensusNotWeighted`] for [`ValidatorServiceKind::RelayConsensus`].
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_points_per_service())]
		pub fn set_points_per_service(
			origin: OriginFor<T>,
			source: T::Source,
			service: ValidatorServiceKind,
			points: u32,
		) -> DispatchResult {
			T::PointsOrigin::ensure_origin(origin)?;
			ensure!(
				service != ValidatorServiceKind::RelayConsensus,
				Error::<T>::RelayConsensusNotWeighted
			);
			ensure!(points <= T::MaxPointsPerService::get(), Error::<T>::PointsPerServiceTooHigh);
			match points {
				0 => PointsPerService::<T>::remove(source, service),
				points => PointsPerService::<T>::insert(source, service, points),
			}
			Self::deposit_event(Event::PointsPerServiceSet { source, service, points });
			Ok(())
		}

		/// Credit the blocks the validators authored on the calling chain.
		///
		/// The weight of crediting is refunded when the calling chain earns no points.
		#[pallet::call_index(1)]
		#[pallet::weight(Pallet::<T>::report_authored_blocks_weight(blocks.len() as u32))]
		pub fn report_authored_blocks(
			origin: OriginFor<T>,
			blocks: Vec<(T::AccountId, u32)>,
		) -> DispatchResultWithPostInfo {
			let source = T::ReportOrigin::ensure_origin(origin)?;
			let credited = Self::credit_authored_blocks(source, blocks);
			Ok(Some(Self::report_authored_blocks_weight(credited)).into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Credit `blocks` authored on `source` as era points, with the points per service of
		/// `source` for [`ValidatorServiceKind::Collation`]. Each count is credited up to
		/// [`Config::MaxBlocksPerEntry`]. Nothing is credited when `blocks` is empty or `source`
		/// earns no points, and an empty `blocks` deposits no event.
		///
		/// Returns the number of entries handed to [`Config::RewardPoints`].
		pub fn credit_authored_blocks(
			source: T::Source,
			blocks: impl IntoIterator<Item = (T::AccountId, u32)>,
		) -> u32 {
			let points_per_service =
				PointsPerService::<T>::get(source, ValidatorServiceKind::Collation)
					.min(T::MaxPointsPerService::get());
			let mut blocks = blocks.into_iter().peekable();
			if blocks.peek().is_none() {
				return 0;
			}
			if points_per_service == 0 {
				Self::deposit_event(Event::BlocksIgnored {
					source,
					entries: blocks.count() as u32,
				});
				return 0;
			}
			let max_blocks = T::MaxBlocksPerEntry::get();
			let points: Vec<_> = blocks
				.map(|(who, blocks)| {
					(who, blocks.min(max_blocks).saturating_mul(points_per_service))
				})
				.collect();
			let entries = points.len() as u32;
			T::RewardPoints::reward(ValidatorServiceKind::Collation, points);
			Self::deposit_event(Event::BlocksCredited { source, entries });
			entries
		}

		/// Weight of [`Self::report_authored_blocks`] when `credited` entries are handed to
		/// [`Config::RewardPoints`].
		pub fn report_authored_blocks_weight(credited: u32) -> Weight {
			let reward = match credited {
				0 => Weight::zero(),
				credited => T::RewardPoints::weight(credited),
			};
			T::WeightInfo::report_authored_blocks().saturating_add(reward)
		}
	}
}
