// This file is part of Substrate.

// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

//! Some helper functions/macros for this crate.

use crate::{
	types::{AllTargetPagesMinerOf, AllVoterPagesMinerOf, MaxWinnersPerPageMinerOf, MinerVoterOf},
	SolutionTargetIndexMinerOf, SolutionVoterIndexMinerOf,
};
use frame_election_provider_support::{PageIndex, VoteWeight};
use frame_support::{traits::Get, BoundedVec};
use sp_runtime::SaturatedConversion;
use sp_std::{cmp::Reverse, collections::btree_map::BTreeMap, vec};

#[macro_export]
macro_rules! log {
	($level:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[#{:?}]:    🎯🎯🎯 🗳  ", $pattern), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

macro_rules! sublog {
	($level:tt, $sub_pallet:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		#[cfg(not(feature = "std"))]
		log!($level, $pattern $(, $values )*);
		#[cfg(feature = "std")]
		log::$level!(
			target: format!("{}::{}", $crate::LOG_TARGET, $sub_pallet).as_ref(),
			concat!("[#{:?}]:     🎯🎯🎯 🗳  ", $pattern), <frame_system::Pallet<T>>::block_number() $(, $values )*
		)
	};
}

use crate::unsigned::miner::Config as MinerConfig;

/// Generate a btree-map cache of the voters and their indices within the provided `snapshot`.
///
/// This does not care about pagination. `snapshot` might be a single page or the entire blob of
/// voters.
///
/// This can be used to efficiently build index getter closures.
pub fn generate_voter_cache<T: MinerConfig, AnyBound: Get<u32>>(
	snapshot: &BoundedVec<MinerVoterOf<T>, AnyBound>,
) -> BTreeMap<T::AccountId, usize> {
	let mut cache: BTreeMap<T::AccountId, usize> = BTreeMap::new();
	snapshot.iter().enumerate().for_each(|(i, (x, _, _))| {
		let _existed = cache.insert(x.clone(), i);
		// if a duplicate exists, we only consider the last one. Defensive only, should never
		// happen.
		debug_assert!(_existed.is_none());
	});

	cache
}

pub fn generate_target_cache<T: MinerConfig>(
	snapshot: &Vec<T::AccountId>,
) -> BTreeMap<T::AccountId, usize> {
	let mut cache: BTreeMap<T::AccountId, usize> = BTreeMap::new();
	snapshot.iter().enumerate().for_each(|(i, x)| {
		let _existed = cache.insert(x.clone(), i);
		// if a duplicate exists, we only consider the last one. Defensive only, should never
		// happen.
		debug_assert!(_existed.is_none());
	});

	cache
}

pub fn generate_voter_staked_cache<T: MinerConfig>(
	snapshot: &Vec<&MinerVoterOf<T>>,
) -> BTreeMap<T::AccountId, usize> {
	let mut cache: BTreeMap<T::AccountId, usize> = BTreeMap::new();
	snapshot.into_iter().enumerate().for_each(|(i, (x, _, _))| {
		let _existed = cache.insert(x.clone(), i);
		// if a duplicate exists, we only consider the last one. Defensive only, should never
		// happen.
		debug_assert!(_existed.is_none());
	});
	cache
}

/// Generate an efficient closure of voters and the page in which they live in.
///
/// The bucketing of voters into a page number is based on their position in the snapshot's page.
pub fn generate_voter_page_fn<T: MinerConfig>(
	paged_snapshot: &AllVoterPagesMinerOf<T>,
) -> impl Fn(&T::AccountId) -> Option<PageIndex> {
	let mut cache: BTreeMap<T::AccountId, PageIndex> = BTreeMap::new();
	paged_snapshot
		.iter()
		.enumerate()
		.map(|(page, whatever)| (page.saturated_into::<PageIndex>(), whatever))
		.for_each(|(page, page_voters)| {
			page_voters.iter().for_each(|(v, _, _)| {
				let _existed = cache.insert(v.clone(), page);
				// if a duplicate exists, we only consider the last one. Defensive only, should
				// never happen.
				debug_assert!(_existed.is_none());
			});
		});

	move |who| cache.get(who).copied()
}

pub fn generate_target_page_fn<T: MinerConfig>(
	paged_snapshot: &AllTargetPagesMinerOf<T>,
) -> impl Fn(&T::AccountId) -> Option<PageIndex> {
	let mut cache: BTreeMap<T::AccountId, PageIndex> = BTreeMap::new();
	paged_snapshot
		.iter()
		.enumerate()
		.map(|(page, whatever)| (page.saturated_into::<PageIndex>(), whatever))
		.for_each(|(page, page_voters)| {
			page_voters.iter().for_each(|t| {
				let _existed = cache.insert(t.clone(), page);
				// if a duplicate exists, we only consider the last one. Defensive only, should
				// never happen.
				debug_assert!(_existed.is_none());
			});
		});
	move |who| cache.get(who).copied()
}

/// stake.
///
/// The bucketing of voters into a page number is based on their relative stake in the assignments
/// set (the larger the stake, the higher the page).
pub fn generate_voter_page_stake_fn<T: MinerConfig>(
	paged_snapshot: &AllVoterPagesMinerOf<T>,
) -> impl Fn(&T::AccountId) -> Option<PageIndex> {
	let mut cache: BTreeMap<T::AccountId, PageIndex> = BTreeMap::new();
	let mut sorted_by_weight: Vec<(VoteWeight, T::AccountId)> = vec![];

	// sort voter stakes.
	let _ = paged_snapshot.to_vec().iter().flatten().for_each(|voter| {
		let pos = sorted_by_weight
			.binary_search_by_key(&Reverse(&voter.1), |(weight, _)| Reverse(weight))
			.unwrap_or_else(|pos| pos);
		sorted_by_weight.insert(pos, (voter.1, voter.0.clone()));
	});

	sorted_by_weight.iter().enumerate().for_each(|(idx, voter)| {
		let page = idx
			.saturating_div(MaxWinnersPerPageMinerOf::<T>::get() as usize)
			.min(T::Pages::get() as usize);
		let _existed = cache.insert(voter.1.clone(), page as PageIndex);
		debug_assert!(_existed.is_none());
	});

	move |who| cache.get(who).copied()
}

/// Create a function that returns the index of a voter in the snapshot.
///
/// The returning index type is the same as the one defined in `T::Solution::Voter`.
///
/// ## Warning
///
/// Note that this will represent the snapshot data from which the `cache` is generated.
pub fn voter_index_fn<T: MinerConfig>(
	cache: &BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> Option<SolutionVoterIndexMinerOf<T>> + '_ {
	move |who| {
		cache
			.get(who)
			.and_then(|i| <usize as TryInto<SolutionVoterIndexMinerOf<T>>>::try_into(*i).ok())
	}
}

/// Create a function that returns the index of a voter in the snapshot.
///
/// Same as [`voter_index_fn`] but the returned function owns all its necessary data; nothing is
/// borrowed.
pub fn voter_index_fn_owned<T: MinerConfig>(
	cache: BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> Option<SolutionVoterIndexMinerOf<T>> {
	move |who| {
		cache
			.get(who)
			.and_then(|i| <usize as TryInto<SolutionVoterIndexMinerOf<T>>>::try_into(*i).ok())
	}
}

pub fn target_index_fn_owned<T: MinerConfig>(
	cache: BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> Option<SolutionTargetIndexMinerOf<T>> {
	move |who| {
		cache
			.get(who)
			.and_then(|i| <usize as TryInto<SolutionTargetIndexMinerOf<T>>>::try_into(*i).ok())
	}
}

/// Create a function that returns the index of a target in the snapshot.
///
/// The returned index type is the same as the one defined in `T::Solution::Target`.
///
/// Note: to the extent possible, the returned function should be cached and reused. Producing that
/// function requires a `O(n log n)` data transform. Each invocation of that function completes
/// in `O(log n)`.
pub fn target_index_fn<T: MinerConfig>(
	snapshot: &Vec<T::AccountId>,
) -> impl Fn(&T::AccountId) -> Option<SolutionTargetIndexMinerOf<T>> + '_ {
	let cache: BTreeMap<_, _> =
		snapshot.iter().enumerate().map(|(idx, account_id)| (account_id, idx)).collect();
	move |who| {
		cache
			.get(who)
			.and_then(|i| <usize as TryInto<SolutionTargetIndexMinerOf<T>>>::try_into(*i).ok())
	}
}

/// Create a function that can map a voter index ([`SolutionVoterIndexOf`]) to the actual voter
/// account using a linearly indexible snapshot.
pub fn voter_at_fn<T: MinerConfig>(
	snapshot: &Vec<MinerVoterOf<T>>,
) -> impl Fn(SolutionVoterIndexMinerOf<T>) -> Option<T::AccountId> + '_ {
	move |i| {
		<SolutionVoterIndexMinerOf<T> as TryInto<usize>>::try_into(i)
			.ok()
			.and_then(|i| snapshot.get(i).map(|(x, _, _)| x).cloned())
	}
}

/// Create a function that can map a target index ([`SolutionTargetIndexOf`]) to the actual target
/// account using a linearly indexible snapshot.
pub fn target_at_fn<T: MinerConfig>(
	snapshot: &Vec<T::AccountId>,
) -> impl Fn(SolutionTargetIndexMinerOf<T>) -> Option<T::AccountId> + '_ {
	move |i| {
		<SolutionTargetIndexMinerOf<T> as TryInto<usize>>::try_into(i)
			.ok()
			.and_then(|i| snapshot.get(i).cloned())
	}
}

/// Same as [`voter_index_fn`], but the returning index is converted into usize, if possible.
///
/// ## Warning
///
/// Note that this will represent the snapshot data from which the `cache` is generated.
pub fn voter_index_fn_usize<T: MinerConfig>(
	cache: &BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> Option<usize> + '_ {
	move |who| cache.get(who).cloned()
}

pub fn target_index_fn_usize<T: MinerConfig>(
	cache: &BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> Option<usize> + '_ {
	move |who| cache.get(who).cloned()
}

/// Create a function to get the stake of a voter.
pub fn stake_of_fn<'a, T: MinerConfig, AnyBound: Get<u32>>(
	snapshot: &'a BoundedVec<MinerVoterOf<T>, AnyBound>,
	cache: &'a BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> VoteWeight + 'a {
	move |who| {
		if let Some(index) = cache.get(who) {
			snapshot.get(*index).map(|(_, x, _)| x).cloned().unwrap_or_default()
		} else {
			0
		}
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use crate::mock::*;

	use sp_runtime::bounded_vec;

	type TargetsPerSnapshotPageOf<T> = <T as crate::Config>::TargetSnapshotPerBlock;

	#[test]
	fn paged_targets_indexing_works() {
		ExtBuilder::default().build_and_execute(|| {
			let all_target_pages: BoundedVec<
				BoundedVec<AccountId, TargetsPerSnapshotPageOf<T>>,
				<T as crate::Config>::Pages,
			> = bounded_vec![
				bounded_vec![],                                // page 0
				bounded_vec![11, 61],                          // page 1
				bounded_vec![51, 101, 31, 41, 21, 81, 71, 91], // page 2
			];

			// flatten all targets.
			let all_targets = all_target_pages.iter().cloned().flatten().collect::<Vec<_>>();

			// `targets_index_fn` get the snapshot page of the target.
			let targets_index_fn = generate_target_page_fn::<T>(&all_target_pages);
			let all_targets_cache = generate_target_cache::<T>(&all_targets);
			let target_index = target_index_fn_usize::<T>(&all_targets_cache);

			// check that the `targets_index_page` is correct.
			for (page, targets) in all_target_pages.iter().enumerate() {
				for t in targets {
					assert_eq!(targets_index_fn(&t), Some(page as PageIndex));
				}
			}

			// check that the `generate_target_cache` ad `target_index_fn_usize` return the correct
			// btree cache/idx.
			for (idx, target) in all_targets.iter().enumerate() {
				assert_eq!(all_targets_cache.get(target), Some(idx).as_ref());
				assert_eq!(target_index(target), Some(idx));
			}
		})
	}
}
