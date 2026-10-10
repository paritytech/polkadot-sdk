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
//! from [`Config::ReportOrigin`], which names the reporting chain as a [`Config::Source`]. This
//! chain credits its own counts through [`Pallet::credit_authored_blocks`]. A count becomes
//! `count × PointsPerBlock[source]` era points, handed to [`Config::RewardPoints`]. A source
//! without an entry in [`PointsPerBlock`] earns nothing. Governance sets the entries through
//! [`Config::PointsOrigin`].

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::weights::Weight;

pub use pallet::*;

/// Credits era points to validators.
pub trait RewardPoints<AccountId> {
	/// Credit `points` to the active era.
	fn reward(points: Vec<(AccountId, u32)>);

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

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Identifies a chain whose blocks earn era points.
		type Source: Parameter + MaxEncodedLen + Ord + Copy + MaybeSerializeDeserialize;

		/// Origin of a chain allowed to report the blocks authored on it. Returns that chain.
		type ReportOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::Source>;

		/// Origin allowed to set the points per block of a source.
		type PointsOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Credits era points to the validators.
		type RewardPoints: RewardPoints<Self::AccountId>;

		/// Maximum number of entries in one report.
		///
		/// Must not be lower than the number of entries a reporting chain sends in one report.
		#[pallet::constant]
		type MaxEntriesPerReport: Get<u32>;

		/// Largest points per block [`Pallet::set_points_per_block`] and the genesis config accept.
		///
		/// Crediting also bounds a stored value by it.
		#[pallet::constant]
		type MaxPointsPerBlock: Get<u32>;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Era points a validator earns per block authored on a source, bounded by
	/// [`Config::MaxPointsPerBlock`] when crediting. Absent means zero.
	#[pallet::storage]
	pub type PointsPerBlock<T: Config> = StorageMap<_, Twox64Concat, T::Source, u32, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial points per block, per source.
		pub points_per_block: Vec<(T::Source, u32)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (source, points) in self.points_per_block.iter().filter(|(_, points)| *points > 0) {
				assert!(
					*points <= T::MaxPointsPerBlock::get(),
					"points per block in the genesis config exceed `MaxPointsPerBlock`"
				);
				PointsPerBlock::<T>::insert(source, points);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Validators earn `points` era points per block authored on `source`.
		PointsPerBlockSet { source: T::Source, points: u32 },
		/// Era points for the blocks of `entries` entries from `source` were handed to
		/// [`Config::RewardPoints`].
		BlocksCredited { source: T::Source, entries: u32 },
		/// The blocks of `entries` entries from `source` earned no points and were not credited.
		BlocksIgnored { source: T::Source, entries: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The points per block exceed [`Config::MaxPointsPerBlock`].
		PointsPerBlockTooHigh,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the era points a validator earns per block authored on `source`.
		///
		/// Zero stops crediting the blocks of `source`. Fails with
		/// [`Error::PointsPerBlockTooHigh`] above [`Config::MaxPointsPerBlock`].
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_points_per_block())]
		pub fn set_points_per_block(
			origin: OriginFor<T>,
			source: T::Source,
			points: u32,
		) -> DispatchResult {
			T::PointsOrigin::ensure_origin(origin)?;
			ensure!(points <= T::MaxPointsPerBlock::get(), Error::<T>::PointsPerBlockTooHigh);
			match points {
				0 => PointsPerBlock::<T>::remove(source),
				points => PointsPerBlock::<T>::insert(source, points),
			}
			Self::deposit_event(Event::PointsPerBlockSet { source, points });
			Ok(())
		}

		/// Credit the blocks the validators authored on the calling chain.
		///
		/// The weight of crediting is refunded when the calling chain earns no points.
		#[pallet::call_index(1)]
		#[pallet::weight(Pallet::<T>::report_authored_blocks_weight(blocks.len() as u32))]
		pub fn report_authored_blocks(
			origin: OriginFor<T>,
			blocks: BoundedVec<(T::AccountId, u32), T::MaxEntriesPerReport>,
		) -> DispatchResultWithPostInfo {
			let source = T::ReportOrigin::ensure_origin(origin)?;
			let credited = Self::credit_authored_blocks(source, blocks);
			Ok(Some(Self::report_authored_blocks_weight(credited)).into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Credit `blocks` authored on `source` as era points, with the points per block of
		/// `source`. Nothing is credited when `blocks` is empty or `source` earns no points, and
		/// an empty `blocks` deposits no event.
		///
		/// Returns the number of entries handed to [`Config::RewardPoints`].
		pub fn credit_authored_blocks(
			source: T::Source,
			blocks: impl IntoIterator<Item = (T::AccountId, u32)>,
		) -> u32 {
			let points_per_block =
				PointsPerBlock::<T>::get(source).min(T::MaxPointsPerBlock::get());
			let mut blocks = blocks.into_iter().peekable();
			if blocks.peek().is_none() {
				return 0;
			}
			if points_per_block == 0 {
				Self::deposit_event(Event::BlocksIgnored {
					source,
					entries: blocks.count() as u32,
				});
				return 0;
			}
			let points: Vec<_> = blocks
				.map(|(who, blocks)| (who, blocks.saturating_mul(points_per_block)))
				.collect();
			let entries = points.len() as u32;
			T::RewardPoints::reward(points);
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
