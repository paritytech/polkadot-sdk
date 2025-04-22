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

//! Primitive traits for providing election functionality.
//!
//! This crate provides two traits that could interact to enable extensible election functionality
//! within FRAME pallets.
//!
//! Something that will provide the functionality of election will implement
//! [`ElectionProvider`], whilst needing an associated [`ElectionProvider::DataProvider`], which
//! needs to be fulfilled by an entity implementing [`ElectionDataProvider`]. Most often, *the data
//! provider is* the receiver of the election, resulting in a diagram as below:
//!
//! ```ignore
//!                                         ElectionDataProvider
//!                          <------------------------------------------+
//!                          |                                          |
//!                          v                                          |
//!                    +-----+----+                              +------+---+
//!                    |          |                              |          |
//! pallet-do-election |          |                              |          | pallet-needs-election
//!                    |          |                              |          |
//!                    |          |                              |          |
//!                    +-----+----+                              +------+---+
//!                          |                                          ^
//!                          |                                          |
//!                          +------------------------------------------+
//!                                         ElectionProvider
//! ```
//!
//! > It could also be possible that a third party pallet (C), provides the data of election to an
//! > election provider (B), which then passes the election result to another pallet (A).
//!
//! ## Election Types
//!
//! Typically, two types of elections exist:
//!
//! 1. **Stateless**: Election data is provided, and the election result is immediately ready.
//! 2. **Stateful**: Election data is is queried ahead of time, and the election result might be
//!    ready some number of blocks in the future.
//!
//! To accommodate both type of elections in one trait, the traits lean toward **stateful
//! election**, as it is more general than the stateless. This is why [`ElectionProvider::elect`]
//! does not receive election data as an input. All value and type parameter must be provided by the
//! [`ElectionDataProvider`] trait, even if the election happens immediately.
//!
//! ## Multi-page election support
//!
//! Both [`ElectionDataProvider`] and [`ElectionProvider`] traits are parameterized by page,
//! supporting an election to be performed over multiple pages. This enables the
//! [`ElectionDataProvider`] implementor to provide all the election data over multiple pages.
//! Similarly [`ElectionProvider::elect`] is parameterized by page index.
//!
//! ## Election Data
//!
//! The data associated with an election, essentially what the [`ElectionDataProvider`] must convey
//! is as follows:
//!
//! 1. A list of voters, with their stake.
//! 2. A list of targets (i.e. _candidates_).
//! 3. A number of desired targets to be elected (i.e. _winners_)
//!
//! In addition to that, the [`ElectionDataProvider`] must also hint [`ElectionProvider`] at when
//! the next election might happen ([`ElectionDataProvider::next_election_prediction`]). A stateless
//! election provider would probably ignore this. A stateful election provider can use this to
//! prepare the election result in advance.
//!
//! Nonetheless, an [`ElectionProvider`] shan't rely on this and should preferably provide some
//! means of fallback election as well, in case the `elect` was called immaturely early.
//!
//! ## Example
//!
//! ```rust
//! # use frame_election_provider_support::{*, data_provider};
//! # use sp_npos_elections::{Support, Assignment};
//! # use frame_support::traits::ConstU32;
//! # use sp_runtime::bounded_vec;
//!
//! type AccountId = u64;
//! type Balance = u64;
//! type BlockNumber = u32;
//!
//! mod data_provider_mod {
//!     use super::*;
//!
//!     pub trait Config: Sized {
//!         type ElectionProvider: ElectionProvider<
//!             AccountId = AccountId,
//!             BlockNumber = BlockNumber,
//!             DataProvider = Pallet<Self>,
//!         >;
//!     }
//!
//!     pub struct Pallet<T: Config>(std::marker::PhantomData<T>);
//!
//!     impl<T: Config> ElectionDataProvider for Pallet<T> {
//!         type AccountId = AccountId;
//!         type BlockNumber = BlockNumber;
//!         type MaxVotesPerVoter = ConstU32<100>;
//!
//!         fn desired_targets() -> data_provider::Result<u32> {
//!             Ok(1)
//!         }
//!         fn electing_voters(bounds: DataProviderBounds, _page: PageIndex)
//!           -> data_provider::Result<Vec<VoterOf<Self>>>
//!         {
//!             Ok(Default::default())
//!         }
//!         fn electable_targets(bounds: DataProviderBounds, _page: PageIndex) -> data_provider::Result<Vec<AccountId>> {
//!             Ok(vec![10, 20, 30])
//!         }
//!         fn next_election_prediction(now: BlockNumber) -> BlockNumber {
//!             0
//!         }
//!     }
//! }
//!
//!
//! mod generic_election_provider {
//!     use super::*;
//!     use sp_runtime::traits::Zero;
//!
//!     pub struct GenericElectionProvider<T: Config>(std::marker::PhantomData<T>);
//!
//!     pub trait Config {
//!         type DataProvider: ElectionDataProvider<AccountId=AccountId, BlockNumber = BlockNumber>;
//!         type MaxWinnersPerPage: Get<u32>;
//!         type MaxBackersPerWinner: Get<u32>;
//!         type Pages: Get<u32>;
//!     }
//!
//!     impl<T: Config> ElectionProvider for GenericElectionProvider<T> {
//!         type AccountId = AccountId;
//!         type BlockNumber = BlockNumber;
//!         type Error = &'static str;
//!         type MaxBackersPerWinner = T::MaxBackersPerWinner;
//!         type MaxWinnersPerPage = T::MaxWinnersPerPage;
//!         type Pages = T::Pages;
//!         type DataProvider = T::DataProvider;
//!
//! 		fn duration() -> <Self as frame_election_provider_support::ElectionProvider>::BlockNumber { todo!() }
//!
//! 		fn start() -> Result<(), <Self as frame_election_provider_support::ElectionProvider>::Error> { todo!() }
//!
//!         fn elect(page: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
//!             unimplemented!()
//!         }
//!
//!         fn status() -> Result<bool, ()> {
//!             unimplemented!()
//!         }
//!     }
//! }
//!
//! mod runtime {
//!     use frame_support::parameter_types;
//!     use super::generic_election_provider;
//!     use super::data_provider_mod;
//!     use super::AccountId;
//!
//!     parameter_types! {
//!         pub static MaxWinnersPerPage: u32 = 10;
//!         pub static MaxBackersPerWinner: u32 = 20;
//!         pub static Pages: u32 = 2;
//!     }
//!
//!     struct Runtime;
//!     impl generic_election_provider::Config for Runtime {
//!         type DataProvider = data_provider_mod::Pallet<Runtime>;
//!         type MaxWinnersPerPage = MaxWinnersPerPage;
//!         type MaxBackersPerWinner = MaxBackersPerWinner;
//!         type Pages = Pages;
//!     }
//!
//!     impl data_provider_mod::Config for Runtime {
//!         type ElectionProvider = generic_election_provider::GenericElectionProvider<Runtime>;
//!     }
//!
//! }
//!
//! # fn main() {}
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

pub mod bounds;
pub mod onchain;
pub mod traits;

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use core::fmt::Debug;
use frame_support::traits::{Defensive, DefensiveResult};
use sp_core::ConstU32;
use sp_runtime::{
	traits::{Bounded, Saturating, Zero},
	RuntimeDebug,
};

pub use bounds::DataProviderBounds;
pub use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
/// Re-export the solution generation macro.
pub use frame_election_provider_solution_type::generate_solution_type;
pub use frame_support::{traits::Get, weights::Weight, BoundedVec, DefaultNoBound};
use scale_info::TypeInfo;
/// Re-export some type as they are used in the interface.
pub use sp_arithmetic::PerThing;
pub use sp_npos_elections::{
	Assignment, BalancingConfig, ElectionResult, Error, ExtendedBalance, IdentifierT, PerThing128,
	Support, Supports, VoteWeight,
};
pub use traits::NposSolution;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

// re-export for the solution macro, with the dependencies of the macro.
#[doc(hidden)]
pub mod private {
	pub use alloc::{collections::btree_set::BTreeSet, vec::Vec};
	pub use codec;
	pub use scale_info;
	pub use sp_arithmetic;

	// Simple Extension trait to easily convert `None` from index closures to `Err`.
	//
	// This is only generated and re-exported for the solution code to use.
	pub trait __OrInvalidIndex<T> {
		fn or_invalid_index(self) -> Result<T, crate::Error>;
	}

	impl<T> __OrInvalidIndex<T> for Option<T> {
		fn or_invalid_index(self) -> Result<T, crate::Error> {
			self.ok_or(crate::Error::SolutionInvalidIndex)
		}
	}
}

use private::__OrInvalidIndex;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// A page index for the multi-block elections pagination.
pub type PageIndex = u32;

/// The [`IndexAssignment`] type is an intermediate between the assignments list
/// ([`&[Assignment<T>]`][Assignment]) and `SolutionOf<T>`.
///
/// The voter and target identifiers have already been replaced with appropriate indices,
/// making it fast to repeatedly encode into a `SolutionOf<T>`. This property turns out
/// to be important when trimming for solution length.
#[derive(RuntimeDebug, Clone, Default)]
#[cfg_attr(feature = "std", derive(PartialEq, Eq, Encode, Decode))]
pub struct IndexAssignment<VoterIndex, TargetIndex, P: PerThing> {
	/// Index of the voter among the voters list.
	pub who: VoterIndex,
	/// The distribution of the voter's stake among winning targets.
	///
	/// Targets are identified by their index in the canonical list.
	pub distribution: Vec<(TargetIndex, P)>,
}

impl<VoterIndex: core::fmt::Debug, TargetIndex: core::fmt::Debug, P: PerThing>
	IndexAssignment<VoterIndex, TargetIndex, P>
{
	pub fn new<AccountId: IdentifierT>(
		assignment: &Assignment<AccountId, P>,
		voter_index: impl Fn(&AccountId) -> Option<VoterIndex>,
		target_index: impl Fn(&AccountId) -> Option<TargetIndex>,
	) -> Result<Self, Error> {
		Ok(Self {
			who: voter_index(&assignment.who).or_invalid_index()?,
			distribution: assignment
				.distribution
				.iter()
				.map(|(target, proportion)| Some((target_index(target)?, *proportion)))
				.collect::<Option<Vec<_>>>()
				.or_invalid_index()?,
		})
	}
}

/// A type alias for [`IndexAssignment`] made from [`NposSolution`].
pub type IndexAssignmentOf<C> = IndexAssignment<
	<C as NposSolution>::VoterIndex,
	<C as NposSolution>::TargetIndex,
	<C as NposSolution>::Accuracy,
>;

/// Types that are used by the data provider trait.
pub mod data_provider {
	/// Alias for the result type of the election data provider.
	pub type Result<T> = core::result::Result<T, &'static str>;
}

/// Something that can provide the data to an [`ElectionProvider`].
pub trait ElectionDataProvider {
	/// The account identifier type.
	type AccountId: Encode;

	/// The block number type.
	type BlockNumber;

	/// Maximum number of votes per voter that this data provider is providing.
	type MaxVotesPerVoter: Get<u32>;

	/// Returns the possible targets for the election associated with the provided `page`, i.e. the
	/// targets that could become elected, thus "electable".
	///
	/// This should be implemented as a self-weighing function. The implementor should register its
	/// appropriate weight at the end of execution with the system pallet directly.
	fn electable_targets(
		bounds: DataProviderBounds,
		page: PageIndex,
	) -> data_provider::Result<Vec<Self::AccountId>>;

	/// A state-less version of [`Self::electable_targets`].
	///
	/// An election-provider that only uses 1 page should use this.
	fn electable_targets_stateless(
		bounds: DataProviderBounds,
	) -> data_provider::Result<Vec<Self::AccountId>> {
		Self::electable_targets(bounds, 0)
	}

	/// All the voters that participate in the election associated with page `page`, thus
	/// "electing".
	///
	/// Note that if a notion of self-vote exists, it should be represented here.
	///
	/// This should be implemented as a self-weighing function. The implementor should register its
	/// appropriate weight at the end of execution with the system pallet directly.
	fn electing_voters(
		bounds: DataProviderBounds,
		page: PageIndex,
	) -> data_provider::Result<Vec<VoterOf<Self>>>;

	/// A state-less version of [`Self::electing_voters`].
	///
	/// An election-provider that only uses 1 page should use this.
	fn electing_voters_stateless(
		bounds: DataProviderBounds,
	) -> data_provider::Result<Vec<VoterOf<Self>>> {
		Self::electing_voters(bounds, 0)
	}

	/// The number of targets to elect.
	///
	/// This should be implemented as a self-weighing function. The implementor should register its
	/// appropriate weight at the end of execution with the system pallet directly.
	///
	/// A sensible implementation should use the minimum between this value and
	/// [`Self::targets().len()`], since desiring a winner set larger than candidates is not
	/// feasible.
	///
	/// This is documented further in issue: <https://github.com/paritytech/substrate/issues/9478>
	fn desired_targets() -> data_provider::Result<u32>;

	/// Provide a best effort prediction about when the next election is about to happen.
	///
	/// In essence, the implementor should predict with this function when it will trigger the
	/// [`ElectionProvider::elect`].
	///
	/// This is only useful for stateful election providers.
	fn next_election_prediction(now: Self::BlockNumber) -> Self::BlockNumber;

	/// Utility function only to be used in benchmarking scenarios, to be implemented optionally,
	/// else a noop.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn put_snapshot(
		_voters: Vec<VoterOf<Self>>,
		_targets: Vec<Self::AccountId>,
		_target_stake: Option<VoteWeight>,
	) {
	}

	/// Instruct the data provider to fetch a page of the solution.
	///
	/// This can be useful to measure the export process in benchmarking.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn fetch_page(_page: PageIndex) {}

	/// Utility function only to be used in benchmarking scenarios, to be implemented optionally,
	/// else a noop.
	///
	/// Same as `put_snapshot`, but can add a single voter one by one.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn add_voter(
		_voter: Self::AccountId,
		_weight: VoteWeight,
		_targets: BoundedVec<Self::AccountId, Self::MaxVotesPerVoter>,
	) {
	}

	/// Utility function only to be used in benchmarking scenarios, to be implemented optionally,
	/// else a noop.
	///
	/// Same as `put_snapshot`, but can add a single voter one by one.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn add_target(_target: Self::AccountId) {}

	/// Clear all voters and targets.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn clear() {}

	/// Force set the desired targets in the snapshot.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn set_desired_targets(_count: u32) {}
}

/// Something that can compute the result of an election and pass it back to the caller in a paged
/// way.
pub trait ElectionProvider {
	/// The account ID identifier;
	type AccountId;

	/// The block number type.
	type BlockNumber;

	/// The error type returned by the provider;
	type Error: Debug + PartialEq;

	/// The maximum number of winners per page in results returned by this election provider.
	///
	/// A winner is an `AccountId` that is part of the final election result.
	type MaxWinnersPerPage: Get<u32>;

	/// The maximum number of backers that a single page may have in results returned by this
	/// election provider.
	///
	/// A backer is an `AccountId` that "backs" one or more winners. For example, in the context of
	/// nominated proof of stake, a backer is a voter that nominates a winner validator in the
	/// election result.
	type MaxBackersPerWinner: Get<u32>;

	/// The number of pages that this election provider supports.
	type Pages: Get<PageIndex>;

	/// The data provider of the election.
	type DataProvider: ElectionDataProvider<
		AccountId = Self::AccountId,
		BlockNumber = Self::BlockNumber,
	>;

	/// Elect a new set of winners.
	///
	/// A complete election may require multiple calls to [`ElectionProvider::elect`] if
	/// [`ElectionProvider::Pages`] is higher than one.
	///
	/// The result is returned in a target major format, namely as vector of supports.
	fn elect(page: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error>;

	/// The index of the *most* significant page that this election provider supports.
	fn msp() -> PageIndex {
		Self::Pages::get().saturating_sub(1)
	}

	/// The index of the *least* significant page that this election provider supports.
	fn lsp() -> PageIndex {
		Zero::zero()
	}

	/// checked call to `Self::DataProvider::desired_targets()` ensuring the value never exceeds
	/// [`Self::MaxWinnersPerPage`].
	fn desired_targets_checked() -> data_provider::Result<u32> {
		Self::DataProvider::desired_targets().and_then(|desired_targets| {
			if desired_targets <= Self::MaxWinnersPerPage::get() {
				Ok(desired_targets)
			} else {
				Err("desired_targets must not be greater than MaxWinners.")
			}
		})
	}

	/// Return the duration of your election.
	///
	/// This excludes the duration of the export. For that, use [`Self::duration_with_export`].
	fn duration() -> Self::BlockNumber;

	/// Return the duration of your election, including the export.
	fn duration_with_export() -> Self::BlockNumber
	where
		Self::BlockNumber: From<PageIndex> + core::ops::Add<Output = Self::BlockNumber>,
	{
		let export: Self::BlockNumber = Self::Pages::get().into();
		Self::duration() + export
	}

	/// Signal that the election should start
	fn start() -> Result<(), Self::Error>;

	/// Indicate whether this election provider is currently ongoing an asynchronous election.
	///
	/// `Err(())` should signal that we are not doing anything, and `elect` should def. not be
	/// called. `Ok(false)` means we are doing something, but work is still ongoing. `elect` should
	/// not be called. `Ok(true)` means we are done and ready for a call to `elect`.
	fn status() -> Result<bool, ()>;

	/// Signal the election provider that we are about to call `elect` asap, and it should prepare
	/// itself.
	#[cfg(feature = "runtime-benchmarks")]
	fn asap() {}
}

/// A (almost) marker trait that signifies an election provider as working synchronously. i.e. being
/// *instant*.
///
/// This must still use the same data provider as with [`ElectionProvider::DataProvider`].
/// However, it can optionally overwrite the amount of voters and targets that are fetched from the
/// data provider at runtime via `forced_input_voters_bound` and `forced_input_target_bound`.
pub trait InstantElectionProvider: ElectionProvider {
	fn instant_elect(
		voters: Vec<VoterOf<Self::DataProvider>>,
		targets: Vec<Self::AccountId>,
		desired_targets: u32,
	) -> Result<BoundedSupportsOf<Self>, Self::Error>;

	// Sine many instant election provider, like [`NoElection`] are meant to do nothing, this is a
	// hint for the caller to call before, and if `false` is returned, not bother with passing all
	// the info to `instant_elect`.
	fn bother() -> bool;
}

/// An election provider that does nothing whatsoever.
pub struct NoElection<X>(core::marker::PhantomData<X>);

impl<AccountId, BlockNumber, DataProvider, MaxWinnersPerPage, MaxBackersPerWinner> ElectionProvider
	for NoElection<(AccountId, BlockNumber, DataProvider, MaxWinnersPerPage, MaxBackersPerWinner)>
where
	DataProvider: ElectionDataProvider<AccountId = AccountId, BlockNumber = BlockNumber>,
	MaxWinnersPerPage: Get<u32>,
	MaxBackersPerWinner: Get<u32>,
	BlockNumber: Zero,
{
	type AccountId = AccountId;
	type BlockNumber = BlockNumber;
	type Error = &'static str;
	type Pages = ConstU32<1>;
	type DataProvider = DataProvider;
	type MaxWinnersPerPage = MaxWinnersPerPage;
	type MaxBackersPerWinner = MaxBackersPerWinner;

	fn elect(_page: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		Err("`NoElection` cannot do anything.")
	}

	fn start() -> Result<(), Self::Error> {
		Err("`NoElection` cannot do anything.")
	}

	fn duration() -> Self::BlockNumber {
		Zero::zero()
	}

	fn status() -> Result<bool, ()> {
		Err(())
	}
}

impl<AccountId, BlockNumber, DataProvider, MaxWinnersPerPage, MaxBackersPerWinner>
	InstantElectionProvider
	for NoElection<(AccountId, BlockNumber, DataProvider, MaxWinnersPerPage, MaxBackersPerWinner)>
where
	DataProvider: ElectionDataProvider<AccountId = AccountId, BlockNumber = BlockNumber>,
	MaxWinnersPerPage: Get<u32>,
	MaxBackersPerWinner: Get<u32>,
	BlockNumber: Zero,
{
	fn instant_elect(
		_: Vec<VoterOf<Self::DataProvider>>,
		_: Vec<Self::AccountId>,
		_: u32,
	) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		Err("`NoElection` cannot do anything.")
	}

	fn bother() -> bool {
		false
	}
}

/// A utility trait for something to implement `ElectionDataProvider` in a sensible way.
///
/// This is generic over `AccountId` and it can represent a validator, a nominator, or any other
/// entity.
///
/// The scores (see [`Self::Score`]) are ascending, the higher, the better.
///
/// Something that implements this trait will do a best-effort sort over ids, and thus can be
/// used on the implementing side of [`ElectionDataProvider`].
pub trait SortedListProvider<AccountId> {
	/// The list's error type.
	type Error: core::fmt::Debug;

	/// The type used by the list to compare nodes for ordering.
	type Score: Bounded + Saturating + Zero + Default;

	/// A typical range for this list.
	///
	/// By default, this would be implemented as `Bounded` impl of `Self::Score`.
	///
	/// If this is implemented by a bags-list instance, it will be the smallest and largest bags.
	///
	/// This is useful to help another pallet that consumes this trait generate an even distribution
	/// of nodes for testing/genesis.
	fn range() -> (Self::Score, Self::Score) {
		(Self::Score::min_value(), Self::Score::max_value())
	}

	/// An iterator over the list, which can have `take` called on it.
	fn iter() -> Box<dyn Iterator<Item = AccountId>>;

	/// Lock the list.
	///
	/// This will prevent subsequent calls to
	/// - [`Self::on_insert`]
	/// - [`Self::on_update`]
	/// - [`Self::on_decrease`]
	/// - [`Self::on_increase`]
	/// - [`Self::on_remove`]
	fn lock();

	/// Unlock the list. This will nullify the effects of [`Self::lock`].
	fn unlock();

	/// Returns an iterator over the list, starting right after from the given voter.
	///
	/// May return an error if `start` is invalid.
	fn iter_from(start: &AccountId) -> Result<Box<dyn Iterator<Item = AccountId>>, Self::Error>;

	/// The current count of ids in the list.
	fn count() -> u32;

	/// Return true if the list already contains `id`.
	fn contains(id: &AccountId) -> bool;

	/// Hook for inserting a new id.
	///
	/// Implementation should return an error if duplicate item is being inserted.
	fn on_insert(id: AccountId, score: Self::Score) -> Result<(), Self::Error>;

	/// Hook for updating a single id.
	///
	/// The `new` score is given.
	///
	/// Returns `Ok(())` iff it successfully updates an item, an `Err(_)` otherwise.
	fn on_update(id: &AccountId, score: Self::Score) -> Result<(), Self::Error>;

	/// Get the score of `id`.
	fn get_score(id: &AccountId) -> Result<Self::Score, Self::Error>;

	/// Same as `on_update`, but incorporate some increased score.
	fn on_increase(id: &AccountId, additional: Self::Score) -> Result<(), Self::Error> {
		let old_score = Self::get_score(id)?;
		let new_score = old_score.saturating_add(additional);
		Self::on_update(id, new_score)
	}

	/// Same as `on_update`, but incorporate some decreased score.
	///
	/// If the new score of the item is `Zero`, it is removed.
	fn on_decrease(id: &AccountId, decreased: Self::Score) -> Result<(), Self::Error> {
		let old_score = Self::get_score(id)?;
		let new_score = old_score.saturating_sub(decreased);
		if new_score.is_zero() {
			Self::on_remove(id)
		} else {
			Self::on_update(id, new_score)
		}
	}

	/// Hook for removing am id from the list.
	///
	/// Returns `Ok(())` iff it successfully removes an item, an `Err(_)` otherwise.
	fn on_remove(id: &AccountId) -> Result<(), Self::Error>;

	/// Regenerate this list from scratch. Returns the count of items inserted.
	///
	/// This should typically only be used at a runtime upgrade.
	///
	/// ## WARNING
	///
	/// This function should be called with care, regenerate will remove the current list write the
	/// new list, which can lead to too many storage accesses, exhausting the block weight.
	fn unsafe_regenerate(
		all: impl IntoIterator<Item = AccountId>,
		score_of: Box<dyn Fn(&AccountId) -> Option<Self::Score>>,
	) -> u32;

	/// Remove all items from the list.
	///
	/// ## WARNING
	///
	/// This function should never be called in production settings because it can lead to an
	/// unbounded amount of storage accesses.
	fn unsafe_clear();

	/// Check internal state of the list. Only meant for debugging.
	#[cfg(feature = "try-runtime")]
	fn try_state() -> Result<(), TryRuntimeError>;

	/// If `who` changes by the returned amount they are guaranteed to have a worst case change
	/// in their list position.
	#[cfg(feature = "runtime-benchmarks")]
	fn score_update_worst_case(_who: &AccountId, _is_increase: bool) -> Self::Score;
}

/// Something that can provide the `Score` of an account. Similar to [`ElectionProvider`] and
/// [`ElectionDataProvider`], this should typically be implementing by whoever is supposed to *use*
/// `SortedListProvider`.
pub trait ScoreProvider<AccountId> {
	type Score;

	/// Get the current `Score` of `who`, `None` if `who` is not present.
	///
	/// `None` can be interpreted as a signal that the voter should be removed from the list.
	fn score(who: &AccountId) -> Option<Self::Score>;

	/// For tests, benchmarks and fuzzing, set the `score`.
	#[cfg(any(feature = "runtime-benchmarks", feature = "fuzz", feature = "std"))]
	fn set_score_of(_: &AccountId, _: Self::Score) {}
}

/// Something that can compute the result to an NPoS solution.
pub trait NposSolver {
	/// The account identifier type of this solver.
	type AccountId: sp_npos_elections::IdentifierT;
	/// The accuracy of this solver. This will affect the accuracy of the output.
	type Accuracy: PerThing128;
	/// The error type of this implementation.
	type Error: core::fmt::Debug + core::cmp::PartialEq;

	/// Solve an NPoS solution with the given `voters`, `targets`, and select `to_elect` count
	/// of `targets`.
	fn solve(
		to_elect: usize,
		targets: Vec<Self::AccountId>,
		voters: Vec<(
			Self::AccountId,
			VoteWeight,
			impl Clone + IntoIterator<Item = Self::AccountId>,
		)>,
	) -> Result<ElectionResult<Self::AccountId, Self::Accuracy>, Self::Error>;

	/// Measure the weight used in the calculation of the solver.
	/// - `voters` is the number of voters.
	/// - `targets` is the number of targets.
	/// - `vote_degree` is the degree ie the maximum numbers of votes per voter.
	fn weight<T: WeightInfo>(voters: u32, targets: u32, vote_degree: u32) -> Weight;
}

/// A quick and dirty solver, that produces a valid but probably worthless election result, but is
/// fast.
///
/// It choses a random number of winners without any consideration.
///
/// Then it iterates over the voters and assigns them to the winners.
///
/// It is only meant to be used in benchmarking.
pub struct QuickDirtySolver<AccountId, Accuracy>(core::marker::PhantomData<(AccountId, Accuracy)>);
impl<AccountId: IdentifierT, Accuracy: PerThing128> NposSolver
	for QuickDirtySolver<AccountId, Accuracy>
{
	type AccountId = AccountId;
	type Accuracy = Accuracy;
	type Error = &'static str;

	fn solve(
		to_elect: usize,
		targets: Vec<Self::AccountId>,
		voters: Vec<(
			Self::AccountId,
			VoteWeight,
			impl Clone + IntoIterator<Item = Self::AccountId>,
		)>,
	) -> Result<ElectionResult<Self::AccountId, Self::Accuracy>, Self::Error> {
		use sp_std::collections::btree_map::BTreeMap;

		if to_elect > targets.len() {
			return Err("to_elect is greater than the number of targets.");
		}

		let winners = targets.into_iter().take(to_elect).collect::<Vec<_>>();

		let mut assignments = Vec::with_capacity(voters.len());
		let mut final_winners = BTreeMap::<Self::AccountId, u128>::new();

		for (voter, weight, votes) in voters {
			let our_winners = winners
				.iter()
				.filter(|w| votes.clone().into_iter().any(|v| v == **w))
				.collect::<Vec<_>>();
			let our_winners_len = our_winners.len();
			let distribution = our_winners
				.into_iter()
				.map(|w| {
					*final_winners.entry(w.clone()).or_default() += weight as u128;
					(w.clone(), Self::Accuracy::from_rational(1, our_winners_len as u128))
				})
				.collect::<Vec<_>>();

			let mut assignment = Assignment { who: voter, distribution };
			assignment.try_normalize().unwrap();
			assignments.push(assignment);
		}

		let winners = final_winners.into_iter().collect::<Vec<_>>();
		Ok(ElectionResult { winners, assignments })
	}

	fn weight<T: WeightInfo>(_: u32, _: u32, _: u32) -> Weight {
		Default::default()
	}
}

/// A wrapper for [`sp_npos_elections::seq_phragmen`] that implements [`NposSolver`]. See the
/// documentation of [`sp_npos_elections::seq_phragmen`] for more info.
pub struct SequentialPhragmen<AccountId, Accuracy, Balancing = ()>(
	core::marker::PhantomData<(AccountId, Accuracy, Balancing)>,
);

impl<AccountId: IdentifierT, Accuracy: PerThing128, Balancing: Get<Option<BalancingConfig>>>
	NposSolver for SequentialPhragmen<AccountId, Accuracy, Balancing>
{
	type AccountId = AccountId;
	type Accuracy = Accuracy;
	type Error = sp_npos_elections::Error;
	fn solve(
		winners: usize,
		targets: Vec<Self::AccountId>,
		voters: Vec<(
			Self::AccountId,
			VoteWeight,
			impl Clone + IntoIterator<Item = Self::AccountId>,
		)>,
	) -> Result<ElectionResult<Self::AccountId, Self::Accuracy>, Self::Error> {
		sp_npos_elections::seq_phragmen(winners, targets, voters, Balancing::get())
	}

	fn weight<T: WeightInfo>(voters: u32, targets: u32, vote_degree: u32) -> Weight {
		T::phragmen(voters, targets, vote_degree)
	}
}

/// A wrapper for [`sp_npos_elections::phragmms()`] that implements [`NposSolver`]. See the
/// documentation of [`sp_npos_elections::phragmms()`] for more info.
pub struct PhragMMS<AccountId, Accuracy, Balancing = ()>(
	core::marker::PhantomData<(AccountId, Accuracy, Balancing)>,
);

impl<AccountId: IdentifierT, Accuracy: PerThing128, Balancing: Get<Option<BalancingConfig>>>
	NposSolver for PhragMMS<AccountId, Accuracy, Balancing>
{
	type AccountId = AccountId;
	type Accuracy = Accuracy;
	type Error = sp_npos_elections::Error;
	fn solve(
		winners: usize,
		targets: Vec<Self::AccountId>,
		voters: Vec<(
			Self::AccountId,
			VoteWeight,
			impl Clone + IntoIterator<Item = Self::AccountId>,
		)>,
	) -> Result<ElectionResult<Self::AccountId, Self::Accuracy>, Self::Error> {
		sp_npos_elections::phragmms(winners, targets, voters, Balancing::get())
	}

	fn weight<T: WeightInfo>(voters: u32, targets: u32, vote_degree: u32) -> Weight {
		T::phragmms(voters, targets, vote_degree)
	}
}

/// A voter, at the level of abstraction of this crate.
pub type Voter<AccountId, Bound> = (AccountId, VoteWeight, BoundedVec<AccountId, Bound>);

/// Same as [`Voter`], but parameterized by an [`ElectionDataProvider`].
pub type VoterOf<D> =
	Voter<<D as ElectionDataProvider>::AccountId, <D as ElectionDataProvider>::MaxVotesPerVoter>;

/// A bounded vector of supports. Bounded equivalent to [`sp_npos_elections::Supports`].
#[derive(
	Default, Debug, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, MaxEncodedLen,
)]
#[codec(mel_bound(AccountId: MaxEncodedLen, Bound: Get<u32>))]
#[scale_info(skip_type_params(Bound))]
pub struct BoundedSupport<AccountId, Bound: Get<u32>> {
	/// Total support.
	pub total: ExtendedBalance,
	/// Support from voters.
	pub voters: BoundedVec<(AccountId, ExtendedBalance), Bound>,
}

impl<AccountId, Bound: Get<u32>> sp_npos_elections::Backings for &BoundedSupport<AccountId, Bound> {
	fn total(&self) -> ExtendedBalance {
		self.total
	}
}

impl<AccountId: PartialEq, Bound: Get<u32>> PartialEq for BoundedSupport<AccountId, Bound> {
	fn eq(&self, other: &Self) -> bool {
		self.total == other.total && self.voters == other.voters
	}
}

impl<AccountId, Bound: Get<u32>> From<BoundedSupport<AccountId, Bound>> for Support<AccountId> {
	fn from(b: BoundedSupport<AccountId, Bound>) -> Self {
		Support { total: b.total, voters: b.voters.into_inner() }
	}
}

impl<AccountId: Clone, Bound: Get<u32>> Clone for BoundedSupport<AccountId, Bound> {
	fn clone(&self) -> Self {
		Self { voters: self.voters.clone(), total: self.total }
	}
}

impl<AccountId, Bound: Get<u32>> TryFrom<sp_npos_elections::Support<AccountId>>
	for BoundedSupport<AccountId, Bound>
{
	type Error = &'static str;
	fn try_from(s: sp_npos_elections::Support<AccountId>) -> Result<Self, Self::Error> {
		let voters = s.voters.try_into().map_err(|_| "voters bound not respected")?;
		Ok(Self { voters, total: s.total })
	}
}

impl<AccountId: Clone, Bound: Get<u32>> BoundedSupport<AccountId, Bound> {
	/// Try and construct a `BoundedSupport` from an unbounded version, and reside to sorting and
	/// truncating if needed.
	///
	/// Returns the number of backers removed.
	pub fn sorted_truncate_from(mut support: sp_npos_elections::Support<AccountId>) -> (Self, u32) {
		// If bounds meet, then short circuit.
		if let Ok(bounded) = support.clone().try_into() {
			return (bounded, 0)
		}

		let pre_len = support.voters.len();
		// sort support based on stake of each backer, low to high.
		// Note: we don't sort high to low and truncate because we would have to track `total`
		// updates, so we need one iteration anyhow.
		support.voters.sort_by(|a, b| a.1.cmp(&b.1));
		// then do the truncation.
		let mut bounded = Self { voters: Default::default(), total: 0 };
		while let Some((voter, weight)) = support.voters.pop() {
			if let Err(_) = bounded.voters.try_push((voter, weight)) {
				break
			}
			bounded.total += weight;
		}
		let post_len = bounded.voters.len();
		(bounded, (pre_len - post_len) as u32)
	}
}

/// A bounded vector of [`BoundedSupport`].
///
/// A [`BoundedSupports`] is a set of [`sp_npos_elections::Supports`] which are bounded in two
/// dimensions. `BInner` corresponds to the bound of the maximum backers per voter and `BOuter`
/// corresponds to the bound of the maximum winners that the bounded supports may contain.
///
/// With the bounds, we control the maximum size of a bounded supports instance.
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, DefaultNoBound, MaxEncodedLen)]
#[codec(mel_bound(AccountId: MaxEncodedLen, BOuter: Get<u32>, BInner: Get<u32>))]
#[scale_info(skip_type_params(BOuter, BInner))]
pub struct BoundedSupports<AccountId, BOuter: Get<u32>, BInner: Get<u32>>(
	pub BoundedVec<(AccountId, BoundedSupport<AccountId, BInner>), BOuter>,
);

/// Try and build yourself from another `BoundedSupports` with a different set of types.
pub trait TryFromOtherBounds<AccountId, BOtherOuter: Get<u32>, BOtherInner: Get<u32>> {
	fn try_from_other_bounds(
		other: BoundedSupports<AccountId, BOtherOuter, BOtherInner>,
	) -> Result<Self, crate::Error>
	where
		Self: Sized;
}

impl<
		AccountId,
		BOuter: Get<u32>,
		BInner: Get<u32>,
		BOtherOuter: Get<u32>,
		BOuterInner: Get<u32>,
	> TryFromOtherBounds<AccountId, BOtherOuter, BOuterInner>
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn try_from_other_bounds(
		other: BoundedSupports<AccountId, BOtherOuter, BOuterInner>,
	) -> Result<Self, crate::Error> {
		// NOTE: we might as well do this with unsafe rust and do it faster.
		if BOtherOuter::get() <= BOuter::get() && BOuterInner::get() <= BInner::get() {
			// Both ouf our bounds are larger than the input's bound, can convert.
			let supports = other
				.into_iter()
				.map(|(acc, b_support)| {
					b_support
						.try_into()
						.defensive_map_err(|_| Error::BoundsExceeded)
						.map(|b_support| (acc, b_support))
				})
				.collect::<Result<Vec<_>, _>>()
				.defensive()?;
			supports.try_into()
		} else {
			Err(crate::Error::BoundsExceeded)
		}
	}
}

impl<AccountId: Clone, BOuter: Get<u32>, BInner: Get<u32>>
	BoundedSupports<AccountId, BOuter, BInner>
{
	/// Try and construct a `BoundedSupports` from an unbounded version, and reside to sorting and
	/// truncating if need ne.
	///
	/// Two u32s returned are number of winners and backers removed respectively.
	pub fn sorted_truncate_from(supports: Supports<AccountId>) -> (Self, u32, u32) {
		// if bounds, meet, short circuit
		if let Ok(bounded) = supports.clone().try_into() {
			return (bounded, 0, 0)
		}

		let pre_winners = supports.len();
		let mut backers_removed = 0;
		// first, convert all inner supports.
		let mut inner_supports = supports
			.into_iter()
			.map(|(account, support)| {
				let (bounded, removed) =
					BoundedSupport::<AccountId, BInner>::sorted_truncate_from(support);
				backers_removed += removed;
				(account, bounded)
			})
			.collect::<Vec<_>>();

		// then sort outer supports based on total stake, high to low
		inner_supports.sort_by(|a, b| b.1.total.cmp(&a.1.total));

		// then take the first slice that can fit.
		let bounded = BoundedSupports(BoundedVec::<
			(AccountId, BoundedSupport<AccountId, BInner>),
			BOuter,
		>::truncate_from(inner_supports));
		let post_winners = bounded.len();
		(bounded, (pre_winners - post_winners) as u32, backers_removed)
	}
}

/// Helper trait for conversion of a vector of unbounded supports into a vector of bounded ones.
pub trait TryFromUnboundedPagedSupports<AccountId, BOuter: Get<u32>, BInner: Get<u32>> {
	fn try_from_unbounded_paged(
		self,
	) -> Result<Vec<BoundedSupports<AccountId, BOuter, BInner>>, crate::Error>
	where
		Self: Sized;
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>>
	TryFromUnboundedPagedSupports<AccountId, BOuter, BInner> for Vec<Supports<AccountId>>
{
	fn try_from_unbounded_paged(
		self,
	) -> Result<Vec<BoundedSupports<AccountId, BOuter, BInner>>, crate::Error> {
		self.into_iter()
			.map(|s| s.try_into().map_err(|_| crate::Error::BoundsExceeded))
			.collect::<Result<Vec<_>, _>>()
	}
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>> sp_npos_elections::EvaluateSupport
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn evaluate(&self) -> sp_npos_elections::ElectionScore {
		sp_npos_elections::evaluate_support(self.iter().map(|(_, s)| s))
	}
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>> sp_std::ops::DerefMut
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl<AccountId: Debug, BOuter: Get<u32>, BInner: Get<u32>> Debug
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		for s in self.0.iter() {
			write!(f, "({:?}, {:?}, {:?}) ", s.0, s.1.total, s.1.voters)?;
		}
		Ok(())
	}
}

impl<AccountId: PartialEq, BOuter: Get<u32>, BInner: Get<u32>> PartialEq
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>> Into<Supports<AccountId>>
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn into(self) -> Supports<AccountId> {
		// NOTE: can be done faster with unsafe code.
		self.0.into_iter().map(|(acc, b_support)| (acc, b_support.into())).collect()
	}
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>>
	From<BoundedVec<(AccountId, BoundedSupport<AccountId, BInner>), BOuter>>
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn from(t: BoundedVec<(AccountId, BoundedSupport<AccountId, BInner>), BOuter>) -> Self {
		Self(t)
	}
}

impl<AccountId: Clone, BOuter: Get<u32>, BInner: Get<u32>> Clone
	for BoundedSupports<AccountId, BOuter, BInner>
{
	fn clone(&self) -> Self {
		Self(self.0.clone())
	}
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>> sp_std::ops::Deref
	for BoundedSupports<AccountId, BOuter, BInner>
{
	type Target = BoundedVec<(AccountId, BoundedSupport<AccountId, BInner>), BOuter>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>> IntoIterator
	for BoundedSupports<AccountId, BOuter, BInner>
{
	type Item = (AccountId, BoundedSupport<AccountId, BInner>);
	type IntoIter = sp_std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		self.0.into_iter()
	}
}

impl<AccountId, BOuter: Get<u32>, BInner: Get<u32>> TryFrom<Supports<AccountId>>
	for BoundedSupports<AccountId, BOuter, BInner>
{
	type Error = crate::Error;

	fn try_from(supports: Supports<AccountId>) -> Result<Self, Self::Error> {
		// optimization note: pre-allocate outer bounded vec.
		let mut outer_bounded_supports = BoundedVec::<
			(AccountId, BoundedSupport<AccountId, BInner>),
			BOuter,
		>::with_bounded_capacity(
			supports.len().min(BOuter::get() as usize)
		);

		// optimization note: avoid intermediate allocations.
		supports
			.into_iter()
			.map(|(account, support)| (account, support.try_into().map_err(|_| ())))
			.try_for_each(|(account, maybe_bounded_supports)| {
				outer_bounded_supports
					.try_push((account, maybe_bounded_supports?))
					.map_err(|_| ())
			})
			.map_err(|_| crate::Error::BoundsExceeded)?;

		Ok(outer_bounded_supports.into())
	}
}

/// Same as `BoundedSupports` but parameterized by an `ElectionProvider`.
pub type BoundedSupportsOf<E> = BoundedSupports<
	<E as ElectionProvider>::AccountId,
	<E as ElectionProvider>::MaxWinnersPerPage,
	<E as ElectionProvider>::MaxBackersPerWinner,
>;

sp_core::generate_feature_enabled_macro!(
	runtime_benchmarks_enabled,
	feature = "runtime-benchmarks",
	$
);

sp_core::generate_feature_enabled_macro!(
	runtime_benchmarks_or_std_enabled,
	any(feature = "runtime-benchmarks", feature = "std"),
	$
);
