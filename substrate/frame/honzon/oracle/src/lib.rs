// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
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

//! # Oracle
//!
//! A pallet that provides a decentralized and trustworthy way to bring external, off-chain data
//! onto the blockchain.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! The Oracle pallet enables blockchain applications to access real-world data through a
//! decentralized network of trusted data providers. It's designed to be flexible and can handle
//! various types of external data such as cryptocurrency prices, weather data, sports scores, or
//! any other off-chain information that needs to be brought on-chain.
//!
//! The pallet operates on a permissioned model where only authorized oracle operators can submit
//! data. This ensures data quality and prevents spam while maintaining decentralization through
//! multiple independent operators. The system aggregates data from multiple sources using
//! configurable algorithms, typically taking the median to resist outliers and manipulation
//! attempts.
//!
//! ### Key Concepts
//!
//! * **Oracle Operators**: A set of trusted accounts authorized to submit data. Managed through the
//!   [`SortedMembers`] trait, allowing integration with membership pallets.
//! * **Data Feeds**: Key-value pairs where keys identify the data type (e.g., currency pair) and
//!   values contain the actual data (e.g., price).
//! * **Data Aggregation**: Configurable algorithms to combine multiple operator inputs into a
//!   single trusted value, with median aggregation provided by default.
//! * **Timestamped Data**: All submitted data includes timestamps for freshness tracking.
//!
//! ## Low Level / Implementation Details
//!
//! ### Design Goals
//!
//! The oracle system aims to provide:
//! - **Decentralization**: Multiple independent data providers prevent single points of failure
//! - **Data Quality**: Aggregation mechanisms filter out outliers and malicious data
//! - **Flexibility**: Configurable data types and aggregation strategies
//! - **Performance**: Efficient storage and retrieval of timestamped data
//! - **Security**: Permissioned access with cryptographic verification of data integrity
//!
//! ### Design
//!
//! The pallet uses a dual-storage approach:
//! - [`RawValues`]: Stores individual operator submissions with timestamps
//! - [`Values`]: Stores the final aggregated values after processing
//!
//! This design allows for:
//! - Historical tracking of individual operator submissions
//! - Efficient access to final aggregated values
//! - Clean separation between raw data and processed results
//! - Easy integration with data aggregation algorithms

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};

use serde::{Deserialize, Serialize};

use frame_support::{
	dispatch::Pays,
	ensure,
	pallet_prelude::*,
	traits::{ChangeMembers, Get, SortedMembers, Time},
	weights::Weight,
	PalletId, Parameter,
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AccountIdConversion, Member},
	DispatchResult, RuntimeDebug,
};
use sp_std::{prelude::*, vec};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod default_combine_data;
pub use default_combine_data::DefaultCombineData;
pub mod traits;
pub use traits::{CombineData, DataFeeder, DataProvider, DataProviderExtended, OnNewData};
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
/// Helper trait for benchmarking oracle operations.
pub trait BenchmarkHelper<OracleKey, OracleValue, L: Get<u32>> {
	/// Returns a list of `(oracle_key, oracle_value)` pairs to be used for
	/// benchmarking.
	///
	/// NOTE: User should ensure to at least submit two values, otherwise the
	/// benchmark linear analysis might fail.
	fn get_currency_id_value_pairs() -> BoundedVec<(OracleKey, OracleValue), L>;
}

#[cfg(feature = "runtime-benchmarks")]
impl<OracleKey, OracleValue, L: Get<u32>> BenchmarkHelper<OracleKey, OracleValue, L> for () {
	fn get_currency_id_value_pairs() -> BoundedVec<(OracleKey, OracleValue), L> {
		BoundedVec::default()
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	pub(crate) type MomentOf<T, I = ()> = <<T as Config<I>>::Time as Time>::Moment;
	pub(crate) type TimestampedValueOf<T, I = ()> =
		TimestampedValue<<T as Config<I>>::OracleValue, MomentOf<T, I>>;

	/// A wrapper for a value with a timestamp.
	#[derive(
		Encode,
		Decode,
		RuntimeDebug,
		Eq,
		PartialEq,
		Clone,
		Copy,
		Ord,
		PartialOrd,
		TypeInfo,
		MaxEncodedLen,
		Serialize,
		Deserialize,
	)]
	pub struct TimestampedValue<Value, Moment> {
		/// The value.
		pub value: Value,
		/// The timestamp.
		pub timestamp: Moment,
	}

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// A hook to be called when new data is received.
		///
		/// This hook is triggered whenever an oracle operator successfully submits new data.
		/// It allows other pallets to react to oracle updates, enabling real-time responses to
		/// external data changes.
		type OnNewData: OnNewData<Self::AccountId, Self::OracleKey, Self::OracleValue>;

		/// The implementation to combine raw values into a single aggregated value.
		///
		/// This type defines how multiple oracle operator submissions are combined into a single
		/// trusted value. Common implementations include taking the median (to resist outliers)
		/// or weighted averages based on operator reputation.
		type CombineData: CombineData<Self::OracleKey, TimestampedValueOf<Self, I>>;

		/// The time provider for timestamping oracle data.
		///
		/// This type provides the current timestamp used to mark when oracle data was submitted.
		/// Timestamps are crucial for determining data freshness and preventing stale data usage.
		type Time: Time;

		/// The key type for identifying oracle data feeds.
		///
		/// This type is used to uniquely identify different types of oracle data (e.g., currency
		/// pairs, asset prices, weather data).
		type OracleKey: Parameter + Member + MaxEncodedLen;

		/// The value type for oracle data.
		///
		/// This type represents the actual data submitted by oracle operators (e.g., prices,
		/// temperatures, scores).
		type OracleValue: Parameter + Member + Ord + MaxEncodedLen;

		/// The pallet ID.
		///
		/// Will be used to derive the pallet's account, which is used as the oracle account
		/// when values are fed by root.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The source of oracle members.
		///
		/// This type provides the set of accounts authorized to submit oracle data.
		/// Typically implemented by membership pallets to allow governance-controlled
		/// management of oracle operators.
		type Members: SortedMembers<Self::AccountId>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The maximum number of oracle operators that can feed data in a single block.
		#[pallet::constant]
		type MaxHasDispatchedSize: Get<u32>;

		/// The maximum number of key-value pairs that can be submitted in a single extrinsic.
		#[pallet::constant]
		type MaxFeedValues: Get<u32>;

		/// A helper trait for benchmarking oracle operations.
		///
		/// Provides sample data for benchmarking the oracle pallet, allowing accurate
		/// weight calculations and performance testing.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<
			Self::OracleKey,
			Self::OracleValue,
			Self::MaxFeedValues,
		>;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The sender is not a member of the oracle and does not have
		/// permission to feed data.
		NoPermission,
		/// The oracle member has already fed data in the current block.
		AlreadyFeeded,
		/// Exceeds the maximum number of `HasDispatched` size.
		ExceedsMaxHasDispatchedSize,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// New data has been fed into the oracle.
		NewFeedData {
			/// The account that fed the data.
			sender: T::AccountId,
			/// The key-value pairs of the data that was fed.
			values: Vec<(T::OracleKey, T::OracleValue)>,
		},
	}

	/// The raw values for each oracle operator.
	///
	/// Maps `(AccountId, OracleKey)` to `TimestampedValue` containing the operator's submitted
	/// value along with the timestamp when it was submitted. This storage maintains the complete
	/// history of individual operator submissions, allowing for data aggregation and audit trails.
	///
	/// ## Storage Economics
	///
	/// No storage deposits are required as this data is considered essential for the oracle's
	/// operation and data integrity. The storage cost is borne by the blockchain as part of the
	/// oracle infrastructure.
	#[pallet::storage]
	#[pallet::getter(fn raw_values)]
	pub type RawValues<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::AccountId,
		Twox64Concat,
		T::OracleKey,
		TimestampedValueOf<T, I>,
	>;

	/// The aggregated values for each oracle key.
	///
	/// Maps `OracleKey` to `TimestampedValue`.
	#[pallet::storage]
	#[pallet::getter(fn values)]
	pub type Values<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, <T as Config<I>>::OracleKey, TimestampedValueOf<T, I>>;

	/// A set of accounts that have already fed data in the current block.
	///
	/// This storage item tracks which oracle operators have already submitted data in the
	/// current block to enforce the "one submission per block" rule. This prevents spam and
	/// ensures fair participation among oracle operators.
	///
	/// The storage is cleared at the end of each block in the `on_finalize` hook, resetting
	/// the state for the next block.
	#[pallet::storage]
	pub(crate) type HasDispatched<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BoundedBTreeSet<T::AccountId, T::MaxHasDispatchedSize>, ValueQuery>;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// `on_initialize` to return the weight used in `on_finalize`.
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			T::WeightInfo::on_finalize()
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			// cleanup for next block
			<HasDispatched<T, I>>::kill();
		}
	}

	#[pallet::view_functions]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Retrieve the aggregated oracle value for a specific key, including its timestamp.
		pub fn get_value(key: T::OracleKey) -> Option<TimestampedValueOf<T, I>> {
			Self::get(&key)
		}

		/// Retrieve every aggregated oracle value tracked by the pallet.
		pub fn all_values() -> Vec<(T::OracleKey, TimestampedValueOf<T, I>)> {
			<Values<T, I>>::iter().collect()
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Feeds external data values into the oracle system.
		///
		/// ## Dispatch Origin
		///
		/// The dispatch origin of this call must be a signed account that is either:
		/// - A member of the oracle operators set (managed by [`SortedMembers`])
		/// - The root origin
		///
		/// ## Details
		///
		/// This function allows authorized oracle operators to submit timestamped key-value pairs
		/// into the oracle system. Each submitted value is immediately timestamped with the current
		/// block time and stored in the [`RawValues`] storage. The system then attempts to
		/// aggregate all raw values for each key using the configured [`CombineData`] trait
		/// implementation, updating the final [`Values`] storage with the aggregated result.
		///
		/// Only one submission per oracle operator per block is allowed to prevent spam and ensure
		/// fair participation. The function also triggers the [`OnNewData`] hook for each submitted
		/// value, allowing other pallets to react to new oracle data.
		///
		/// ## Errors
		///
		/// - [`Error::NoPermission`]: The sender is not authorized to feed data
		/// - [`Error::AlreadyFeeded`]: The sender has already fed data in the current block
		/// - [`Error::ExceedsMaxHasDispatchedSize`]: Too many operators have fed data in this block
		///
		/// ## Events
		///
		/// - [`Event::NewFeedData`]: Emitted when data is successfully fed into the oracle
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::feed_values(values.len() as u32))]
		pub fn feed_values(
			origin: OriginFor<T>,
			values: BoundedVec<(T::OracleKey, T::OracleValue), T::MaxFeedValues>,
		) -> DispatchResultWithPostInfo {
			let feeder = ensure_signed_or_root(origin.clone())?;

			let who = Self::ensure_account(feeder)?;

			// ensure account hasn't dispatched an updated yet
			<HasDispatched<T, I>>::try_mutate(|set| {
				set.try_insert(who.clone())
					.map_err(|_| Error::<T, I>::ExceedsMaxHasDispatchedSize)?
					.then_some(())
					.ok_or(Error::<T, I>::AlreadyFeeded)
			})?;

			Self::do_feed_values(who, values.into());
			Ok(Pays::No.into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn get_pallet_account() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}

	/// Reads the raw values for a given key from all oracle members.
	pub fn read_raw_values(key: &T::OracleKey) -> Vec<TimestampedValueOf<T, I>> {
		T::Members::sorted_members()
			.iter()
			.chain([Self::get_pallet_account()].iter())
			.filter_map(|x| Self::raw_values(x, key))
			.collect()
	}

	/// Returns the aggregated and timestamped value for a given key.
	pub fn get(key: &T::OracleKey) -> Option<TimestampedValueOf<T, I>> {
		Self::values(key)
	}

	fn combined(key: &T::OracleKey) -> Option<TimestampedValueOf<T, I>> {
		let values = Self::read_raw_values(key);
		T::CombineData::combine_data(key, values, Self::values(key))
	}

	fn ensure_account(who: Option<T::AccountId>) -> Result<T::AccountId, DispatchError> {
		// ensure feeder is authorized
		if let Some(who) = who {
			ensure!(T::Members::contains(&who), Error::<T, I>::NoPermission);
			Ok(who)
		} else {
			Ok(Self::get_pallet_account())
		}
	}

	fn do_feed_values(who: T::AccountId, values: Vec<(T::OracleKey, T::OracleValue)>) {
		let now = T::Time::now();
		for (key, value) in &values {
			let timestamped = TimestampedValue { value: value.clone(), timestamp: now };
			RawValues::<T, I>::insert(&who, key, timestamped);

			// Update `Values` storage if `combined` yielded result.
			if let Some(combined) = Self::combined(key) {
				<Values<T, I>>::insert(key, combined);
			}

			T::OnNewData::on_new_data(&who, key, value);
		}
		Self::deposit_event(Event::NewFeedData { sender: who, values });
	}
}

impl<T: Config<I>, I: 'static> ChangeMembers<T::AccountId> for Pallet<T, I> {
	fn change_members_sorted(
		_incoming: &[T::AccountId],
		outgoing: &[T::AccountId],
		_new: &[T::AccountId],
	) {
		// remove values
		for removed in outgoing {
			let _ = RawValues::<T, I>::clear_prefix(removed, u32::MAX, None);
		}
	}

	fn set_prime(_prime: Option<T::AccountId>) {
		// nothing
	}
}

impl<T: Config<I>, I: 'static> DataProvider<T::OracleKey, T::OracleValue> for Pallet<T, I> {
	fn get(key: &T::OracleKey) -> Option<T::OracleValue> {
		Self::get(key).map(|timestamped_value| timestamped_value.value)
	}
}
impl<T: Config<I>, I: 'static> DataProviderExtended<T::OracleKey, TimestampedValueOf<T, I>>
	for Pallet<T, I>
{
	fn get_all_values() -> impl Iterator<Item = (T::OracleKey, Option<TimestampedValueOf<T, I>>)> {
		<Values<T, I>>::iter().map(|(k, v)| (k, Some(v)))
	}
}

impl<T: Config<I>, I: 'static> DataFeeder<T::OracleKey, T::OracleValue, T::AccountId>
	for Pallet<T, I>
{
	fn feed_value(
		who: Option<T::AccountId>,
		key: T::OracleKey,
		value: T::OracleValue,
	) -> DispatchResult {
		Self::do_feed_values(Self::ensure_account(who)?, vec![(key, value)]);
		Ok(())
	}
}
