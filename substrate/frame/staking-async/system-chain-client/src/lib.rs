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

//! The client for system chains, intended to be used in AssetHub.
//!
//! Credits the blocks validators author on system chains as era points. Meant for the chain that
//! runs `pallet-staking-async`.
//!
//! ## Overview
//!
//! Each system chain counts the blocks its authors produce in `pallet-validator-collators` and
//! reports the counts of the validators. Other chains call [`Pallet::report_authored_blocks`]
//! from [`Config::ReportOrigin`], which names the reporting chain as a [`Config::Source`]. The
//! chain that runs this pallet credits its own counts through [`Pallet::credit_authored_blocks`].
//! A count becomes `count × PointsPerUnit[(source, Collation)]` era points, handed to
//! [`Config::RewardPoints`] as [`PointsKind::Collation`] points. A source without an entry for
//! [`PointsKind::Collation`] in [`PointsPerUnit`] earns nothing. Governance sets the entries
//! through [`Config::PointsOrigin`]. Relay chain validation points are not weighted here.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::weights::Weight;
use sp_staking::PointsKind;

pub use pallet::*;

/// Credits era points to validators.
pub trait RewardPoints<AccountId> {
	/// Credit `points` earned for `kind` to the active era.
	fn reward(kind: PointsKind, points: Vec<(AccountId, u32)>);

	/// Weight of [`Self::reward`] for `entries` entries.
	fn weight(entries: u32) -> Weight;
}

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
	use crate::RewardPoints;
	use alloc::vec::Vec;
	use frame_support::{pallet_prelude::*, DefaultNoBound};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::MaybeSerializeDeserialize;
	use sp_staking::PointsKind;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Identifies a chain whose blocks earn era points.
		type Source: Parameter + MaxEncodedLen + Ord + Copy + MaybeSerializeDeserialize;

		/// Origin of a chain allowed to report the blocks authored on it. Returns that chain.
		type ReportOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::Source>;

		/// Origin allowed to set the points per unit of a source and a service.
		type PointsOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Credits era points to the validators.
		type RewardPoints: RewardPoints<Self::AccountId>;

		/// Largest points per unit [`Pallet::set_points_per_unit`] and the genesis config accept,
		/// for every service.
		///
		/// Crediting also bounds a stored value by it.
		#[pallet::constant]
		type MaxPointsPerUnit: Get<u32>;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Era points a validator earns per unit of a service on a source, bounded by
	/// [`Config::MaxPointsPerUnit`] when crediting. Absent means zero.
	///
	/// A unit is specific to the service. For [`PointsKind::Collation`] it is one block authored.
	#[pallet::storage]
	pub type PointsPerUnit<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::Source, Twox64Concat, PointsKind, u32, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial points per unit, per source and service.
		pub points_per_unit: Vec<(T::Source, PointsKind, u32)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (source, kind, points) in
				self.points_per_unit.iter().filter(|(_, _, points)| *points > 0)
			{
				assert!(
					*kind != PointsKind::Validation,
					"the genesis config weights validation points, which come from the relay chain"
				);
				assert!(
					*points <= T::MaxPointsPerUnit::get(),
					"points per unit in the genesis config exceed `MaxPointsPerUnit`"
				);
				PointsPerUnit::<T>::insert(source, kind, points);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Validators earn `points` era points per unit of `kind` on `source`.
		PointsPerUnitSet { source: T::Source, kind: PointsKind, points: u32 },
		/// Era points for the blocks of `entries` entries from `source` were handed to
		/// [`Config::RewardPoints`].
		BlocksCredited { source: T::Source, entries: u32 },
		/// The blocks of `entries` entries from `source` earned no points and were not credited.
		BlocksIgnored { source: T::Source, entries: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The points per unit exceed [`Config::MaxPointsPerUnit`].
		PointsPerUnitTooHigh,
		/// Validation points are weighted by the relay chain, not by this pallet.
		ValidationNotWeighted,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the era points a validator earns per unit of `kind` on `source`.
		///
		/// Zero removes the entry, so `source` earns nothing for `kind`. Fails with
		/// [`Error::PointsPerUnitTooHigh`] above [`Config::MaxPointsPerUnit`], and with
		/// [`Error::ValidationNotWeighted`] for [`PointsKind::Validation`].
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_points_per_unit())]
		pub fn set_points_per_unit(
			origin: OriginFor<T>,
			source: T::Source,
			kind: PointsKind,
			points: u32,
		) -> DispatchResult {
			T::PointsOrigin::ensure_origin(origin)?;
			ensure!(kind != PointsKind::Validation, Error::<T>::ValidationNotWeighted);
			ensure!(points <= T::MaxPointsPerUnit::get(), Error::<T>::PointsPerUnitTooHigh);
			match points {
				0 => PointsPerUnit::<T>::remove(source, kind),
				points => PointsPerUnit::<T>::insert(source, kind, points),
			}
			Self::deposit_event(Event::PointsPerUnitSet { source, kind, points });
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
		/// Credit `blocks` authored on `source` as era points, with the points per unit of
		/// `source` for [`PointsKind::Collation`]. Nothing is credited when `blocks` is empty or
		/// `source` earns no points, and an empty `blocks` deposits no event.
		///
		/// Returns the number of entries handed to [`Config::RewardPoints`].
		pub fn credit_authored_blocks(
			source: T::Source,
			blocks: impl IntoIterator<Item = (T::AccountId, u32)>,
		) -> u32 {
			let points_per_unit = PointsPerUnit::<T>::get(source, PointsKind::Collation)
				.min(T::MaxPointsPerUnit::get());
			let mut blocks = blocks.into_iter().peekable();
			if blocks.peek().is_none() {
				return 0;
			}
			if points_per_unit == 0 {
				Self::deposit_event(Event::BlocksIgnored {
					source,
					entries: blocks.count() as u32,
				});
				return 0;
			}
			let points: Vec<_> = blocks
				.map(|(who, blocks)| (who, blocks.saturating_mul(points_per_unit)))
				.collect();
			let entries = points.len() as u32;
			T::RewardPoints::reward(PointsKind::Collation, points);
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
