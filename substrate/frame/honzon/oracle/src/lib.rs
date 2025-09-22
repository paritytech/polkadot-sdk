//! # Oracle Module
//!
//! ## Overview
//!
//! The Oracle module provides a decentralized and trustworthy way to bring
//! external, off-chain data onto the blockchain. It allows a configurable set of
//! oracle operators to feed data, such as prices, into the system. This data can
//! then be used by other pallets.
//!
//! The module is designed to be flexible and can be configured to use different
//! data sources and aggregation strategies.
//!
//! ### Key Concepts
//!
//! * **Oracle Operators**: A set of trusted accounts that are authorized to submit data to the
//!   oracle. The module uses the `frame_support::traits::SortedMembers` trait to manage the set of
//!   operators. This allows using pallets like `pallet-membership` to manage the oracle members.
//! * **Data Feeds**: Operators feed data as key-value pairs. The `OracleKey` is used to identify
//!   the data being fed (e.g., a specific currency pair), and the `OracleValue` is the data itself
//!   (e.g., the price).
//! * **Data Aggregation**: The module can be configured with a `CombineData` implementation to
//!   aggregate the raw values submitted by individual operators into a single, trusted value. A
//!   default implementation `DefaultCombineData` is provided, which takes the median of the values.
//! * **Timestamped Data**: All data submitted to the oracle is timestamped, allowing consumers of
//!   the data to know how fresh it is.

#![cfg_attr(not(feature = "std"), no_std)]
// Disable the following two lints since they originate from an external macro (namely decl_storage)
#![allow(clippy::string_lit_as_bytes)]
#![allow(clippy::unused_unit)]

use codec::{Decode, Encode, MaxEncodedLen};

use serde::{Deserialize, Serialize};

use frame_support::{
	dispatch::Pays,
	ensure,
	pallet_prelude::*,
	traits::{ChangeMembers, Get, SortedMembers, Time},
	weights::Weight,
	Parameter,
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::{traits::Member, DispatchResult, RuntimeDebug};
use sp_std::{prelude::*, vec};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod default_combine_data;
pub use default_combine_data::DefaultCombineData;
mod traits;
pub use traits::{CombineData, DataFeeder, DataProvider, DataProviderExtended, OnNewData};
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
/// Helper trait for benchmarking.
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
		type OnNewData: OnNewData<Self::AccountId, Self::OracleKey, Self::OracleValue>;

		/// The implementation to combine raw values into a single aggregated
		/// value.
		type CombineData: CombineData<Self::OracleKey, TimestampedValueOf<Self, I>>;

		/// The time provider.
		type Time: Time;

		/// The key type for the oracle data.
		type OracleKey: Parameter + Member + MaxEncodedLen;

		/// The value type for the oracle data.
		type OracleValue: Parameter + Member + Ord + MaxEncodedLen;

		/// The account ID for the root operator. This account can bypass the
		/// membership check and feed values directly.
		#[pallet::constant]
		type RootOperatorAccountId: Get<Self::AccountId>;

		/// The source of oracle members.
		type Members: SortedMembers<Self::AccountId>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The maximum number of members that can be stored in `HasDispatched`.
		#[pallet::constant]
		type MaxHasDispatchedSize: Get<u32>;

		/// The maximum number of values that can be fed in a single extrinsic.
		#[pallet::constant]
		type MaxFeedValues: Get<u32>;

		/// A helper trait for benchmarking.
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
	/// Maps `(AccountId, OracleKey)` to `TimestampedValue`.
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

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Feed the external value.
		///
		/// This function can be called by any authorized oracle member to feed
		/// a list of key-value pairs. The `origin` of the transaction must be a
		/// member of the oracle or the root operator.
		///
		/// - `values`: A list of key-value pairs to be fed into the oracle.
		///
		/// Emits a `NewFeedData` event on success.
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

			Self::do_feed_values(who, values.into())?;
			Ok(Pays::No.into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Reads the raw values for a given key from all oracle members.
	pub fn read_raw_values(key: &T::OracleKey) -> Vec<TimestampedValueOf<T, I>> {
		T::Members::sorted_members()
			.iter()
			.chain([T::RootOperatorAccountId::get()].iter())
			.filter_map(|x| Self::raw_values(x, key))
			.collect()
	}

	/// Returns the aggregated and timestamped value for a given key.
	pub fn get(key: &T::OracleKey) -> Option<TimestampedValueOf<T, I>> {
		Self::values(key)
	}

	/// Returns all aggregated and timestamped values.
	#[allow(clippy::complexity)]
	pub fn get_all_values() -> Vec<(T::OracleKey, Option<TimestampedValueOf<T, I>>)> {
		<Values<T, I>>::iter().map(|(k, v)| (k, Some(v))).collect()
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
			Ok(T::RootOperatorAccountId::get())
		}
	}

	fn do_feed_values(
		who: T::AccountId,
		values: Vec<(T::OracleKey, T::OracleValue)>,
	) -> DispatchResult {
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
		Ok(())
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
	fn get_no_op(key: &T::OracleKey) -> Option<TimestampedValueOf<T, I>> {
		Self::get(key)
	}

	#[allow(clippy::complexity)]
	fn get_all_values() -> Vec<(T::OracleKey, Option<TimestampedValueOf<T, I>>)> {
		Self::get_all_values()
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
		Self::do_feed_values(Self::ensure_account(who)?, vec![(key, value)])
	}
}
