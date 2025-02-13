// This file is part of Substrate.

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

//! Some helper functions/macros for this crate.

use crate::{
	types::{PageIndex, VoterOf},
	unsigned::miner::MinerConfig,
	AllVoterPagesOf, SolutionTargetIndexOf, SolutionVoterIndexOf, VoteWeight,
};
use frame_support::{traits::Get, BoundedVec};
use sp_runtime::SaturatedConversion;
use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, prelude::*};

/// Emit a log specific to this pallet, setting the target to [`crate::LOG_PREFIX`]
#[macro_export]
macro_rules! log {
	($level:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_PREFIX,
			concat!("[#{:?}] 🗳🗳🗳  ", $pattern), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// Emit a log within a submodule of the pallet
#[macro_export]
macro_rules! sublog {
	($level:tt, $sub_pallet:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		#[cfg(not(feature = "std"))]
		log!($level, $pattern $(, $values )*);
		#[cfg(feature = "std")]
		log::$level!(
			target: format!("{}::{}", $crate::LOG_PREFIX, $sub_pallet).as_ref(),
			concat!("[#{:?}] 🗳🗳🗳  ", $pattern), <frame_system::Pallet<T>>::block_number() $(, $values )*
		)
	};
}

/// Emit a log from within the offchain miner.
#[macro_export]
macro_rules! miner_log {
	($level:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_PREFIX,
			concat!("[⛏️miner] 🗳🗳🗳  ", $pattern) $(, $values)*
		)
	};
}

/// Generate an `efficient closure of voters and the page in which they live in.
pub(crate) fn generate_voter_page_fn<T: MinerConfig>(
	paged_snapshot: &AllVoterPagesOf<T>,
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

/// Generate a btree-map cache of the voters and their indices within the provided `snapshot`.
///
/// This does not care about pagination. `snapshot` might be a single page or the entire blob of
/// voters.
///
/// This can be used to efficiently build index getter closures.
pub(crate) fn generate_voter_cache<T: MinerConfig, AnyBound: Get<u32>>(
	snapshot: &BoundedVec<VoterOf<T>, AnyBound>,
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

/// Create a function that returns the index of a voter in the snapshot.
///
/// Same as [`voter_index_fn`] but the returned function owns all its necessary data; nothing is
/// borrowed.
pub(crate) fn voter_index_fn_owned<T: MinerConfig>(
	cache: BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> Option<SolutionVoterIndexOf<T>> {
	move |who| {
		cache
			.get(who)
			.and_then(|i| <usize as TryInto<SolutionVoterIndexOf<T>>>::try_into(*i).ok())
	}
}

/// Same as [`voter_index_fn`], but the returning index is converted into usize, if possible.
///
/// ## Warning
///
/// Note that this will represent the snapshot data from which the `cache` is generated.
pub(crate) fn voter_index_fn_usize<T: MinerConfig>(
	cache: &BTreeMap<T::AccountId, usize>,
) -> impl Fn(&T::AccountId) -> Option<usize> + '_ {
	move |who| cache.get(who).cloned()
}

/// A non-optimized, linear version of [`voter_index_fn`] that does not need a cache and does a
/// linear search.
///
/// ## Warning
///
/// Not meant to be used in production.
#[cfg(test)]
pub(crate) fn voter_index_fn_linear<T: MinerConfig>(
	snapshot: &Vec<VoterOf<T>>,
) -> impl Fn(&T::AccountId) -> Option<SolutionVoterIndexOf<T>> + '_ {
	move |who| {
		snapshot
			.iter()
			.position(|(x, _, _)| x == who)
			.and_then(|i| <usize as TryInto<SolutionVoterIndexOf<T>>>::try_into(i).ok())
	}
}

/// Create a function that returns the index of a target in the snapshot.
///
/// The returned index type is the same as the one defined in `T::Solution::Target`.
///
/// Note: to the extent possible, the returned function should be cached and reused. Producing that
/// function requires a `O(n log n)` data transform. Each invocation of that function completes
/// in `O(log n)`.
pub(crate) fn target_index_fn<T: MinerConfig>(
	snapshot: &Vec<T::AccountId>,
) -> impl Fn(&T::AccountId) -> Option<SolutionTargetIndexOf<T>> + '_ {
	let cache: BTreeMap<_, _> =
		snapshot.iter().enumerate().map(|(idx, account_id)| (account_id, idx)).collect();
	move |who| {
		cache
			.get(who)
			.and_then(|i| <usize as TryInto<SolutionTargetIndexOf<T>>>::try_into(*i).ok())
	}
}

/// Create a function the returns the index to a target in the snapshot.
///
/// The returned index type is the same as the one defined in `T::Solution::Target`.
///
/// ## Warning
///
/// Not meant to be used in production.
#[cfg(test)]
pub(crate) fn target_index_fn_linear<T: MinerConfig>(
	snapshot: &Vec<T::AccountId>,
) -> impl Fn(&T::AccountId) -> Option<SolutionTargetIndexOf<T>> + '_ {
	move |who| {
		snapshot
			.iter()
			.position(|x| x == who)
			.and_then(|i| <usize as TryInto<SolutionTargetIndexOf<T>>>::try_into(i).ok())
	}
}

/// Create a function that can map a voter index ([`SolutionVoterIndexOf`]) to the actual voter
/// account using a linearly indexible snapshot.
pub(crate) fn voter_at_fn<T: MinerConfig>(
	snapshot: &Vec<VoterOf<T>>,
) -> impl Fn(SolutionVoterIndexOf<T>) -> Option<T::AccountId> + '_ {
	move |i| {
		<SolutionVoterIndexOf<T> as TryInto<usize>>::try_into(i)
			.ok()
			.and_then(|i| snapshot.get(i).map(|(x, _, _)| x).cloned())
	}
}

/// Create a function that can map a target index ([`SolutionTargetIndexOf`]) to the actual target
/// account using a linearly indexible snapshot.
pub(crate) fn target_at_fn<T: MinerConfig>(
	snapshot: &Vec<T::AccountId>,
) -> impl Fn(SolutionTargetIndexOf<T>) -> Option<T::AccountId> + '_ {
	move |i| {
		<SolutionTargetIndexOf<T> as TryInto<usize>>::try_into(i)
			.ok()
			.and_then(|i| snapshot.get(i).cloned())
	}
}

/// Create a function to get the stake of a voter.
///
/// ## Warning
///
/// The cache need must be derived from the same snapshot. Zero is returned if a voter is
/// non-existent.
pub(crate) fn stake_of_fn<'a, T: MinerConfig, AnyBound: Get<u32>>(
	snapshot: &'a BoundedVec<VoterOf<T>, AnyBound>,
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
