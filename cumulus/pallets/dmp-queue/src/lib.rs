// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Pallet implementing a message queue for downward messages from the relay-chain.
//! Executes downward messages if there is enough weight available and schedules the rest for later
//! execution (by `on_idle` or another `handle_dmp_messages` call). Individual overweight messages
//! are scheduled into a separate queue that is only serviced by explicit extrinsic calls.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod migration;

use codec::{Decode, DecodeLimit, Encode};
use cumulus_primitives_core::{relay_chain::BlockNumber as RelayBlockNumber, DmpMessageHandler};
use frame_support::{
	traits::EnsureOrigin,
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};
pub use pallet::*;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{convert::TryFrom, prelude::*};
use xcm::{latest::prelude::*, VersionedXcm, MAX_XCM_DECODE_DEPTH};

const DEFAULT_POV_SIZE: u64 = 64 * 1024; // 64 KB

// Maximum amount of messages to process per block. This is a temporary measure until we properly
// account for proof size weights.
const MAX_MESSAGES_PER_BLOCK: u8 = 10;
// Maximum amount of messages that can exist in the overweight queue at any given time.
const MAX_OVERWEIGHT_MESSAGES: u32 = 1000;

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ConfigData {
	/// The maximum amount of weight any individual message may consume. Messages above this weight
	/// go into the overweight queue and may only be serviced explicitly by the
	/// `ExecuteOverweightOrigin`.
	max_individual: Weight,
}

impl Default for ConfigData {
	fn default() -> Self {
		Self {
			max_individual: Weight::from_parts(
				10u64 * WEIGHT_REF_TIME_PER_MILLIS, // 10 ms of execution time maximum by default
				DEFAULT_POV_SIZE,                   // 64 KB of proof size by default
			),
		}
	}
}

/// Information concerning our message pages.
#[derive(Copy, Clone, Eq, PartialEq, Default, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct PageIndexData {
	/// The lowest used page index.
	begin_used: PageCounter,
	/// The lowest unused page index.
	end_used: PageCounter,
	/// The number of overweight messages ever recorded (and thus the lowest free index).
	overweight_count: OverweightIndex,
}

/// Simple type used to identify messages for the purpose of reporting events. Secure if and only
/// if the message content is unique.
pub type MessageId = XcmHash;

/// Index used to identify overweight messages.
pub type OverweightIndex = u64;

/// Index used to identify normal pages.
pub type PageCounter = u32;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::storage_version(migration::STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type XcmExecutor: ExecuteXcm<Self::RuntimeCall>;

		/// Origin which is allowed to execute overweight messages.
		type ExecuteOverweightOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	/// The configuration.
	#[pallet::storage]
	pub(super) type Configuration<T> = StorageValue<_, ConfigData, ValueQuery>;

	/// The page index.
	#[pallet::storage]
	pub(super) type PageIndex<T> = StorageValue<_, PageIndexData, ValueQuery>;

	/// The queue pages.
	#[pallet::storage]
	pub(super) type Pages<T> =
		StorageMap<_, Blake2_128Concat, PageCounter, Vec<(RelayBlockNumber, Vec<u8>)>, ValueQuery>;

	/// The overweight messages.
	#[pallet::storage]
	pub(super) type Overweight<T> = CountedStorageMap<
		_,
		Blake2_128Concat,
		OverweightIndex,
		(RelayBlockNumber, Vec<u8>),
		OptionQuery,
	>;

	#[pallet::error]
	pub enum Error<T> {
		/// The message index given is unknown.
		Unknown,
		/// The amount of weight given is possibly not enough for executing the message.
		OverLimit,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_now: BlockNumberFor<T>, max_weight: Weight) -> Weight {
			// on_idle processes additional messages with any remaining block weight.
			Self::service_queue(max_weight)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Service a single overweight message.
		#[pallet::call_index(0)]
		#[pallet::weight(weight_limit.saturating_add(Weight::from_parts(1_000_000, 0)))]
		pub fn service_overweight(
			origin: OriginFor<T>,
			index: OverweightIndex,
			weight_limit: Weight,
		) -> DispatchResultWithPostInfo {
			T::ExecuteOverweightOrigin::ensure_origin(origin)?;

			let (sent_at, data) = Overweight::<T>::get(index).ok_or(Error::<T>::Unknown)?;
			let weight_used = Self::try_service_message(weight_limit, sent_at, &data[..])
				.map_err(|_| Error::<T>::OverLimit)?;
			Overweight::<T>::remove(index);
			Self::deposit_event(Event::OverweightServiced { overweight_index: index, weight_used });
			Ok(Some(weight_used.saturating_add(Weight::from_parts(1_000_000, 0))).into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Downward message is invalid XCM.
		InvalidFormat { message_hash: XcmHash },
		/// Downward message is unsupported version of XCM.
		UnsupportedVersion { message_hash: XcmHash },
		/// Downward message executed with the given outcome.
		ExecutedDownward { message_hash: XcmHash, message_id: XcmHash, outcome: Outcome },
		/// The weight limit for handling downward messages was reached.
		WeightExhausted {
			message_hash: XcmHash,
			message_id: XcmHash,
			remaining_weight: Weight,
			required_weight: Weight,
		},
		/// Downward message is overweight and was placed in the overweight queue.
		OverweightEnqueued {
			message_hash: XcmHash,
			message_id: XcmHash,
			overweight_index: OverweightIndex,
			required_weight: Weight,
		},
		/// Downward message from the overweight queue was executed.
		OverweightServiced { overweight_index: OverweightIndex, weight_used: Weight },
		/// The maximum number of downward messages was reached.
		MaxMessagesExhausted { message_hash: XcmHash },
	}

	/// Error type when a message was failed to be serviced.
	pub(crate) struct ServiceMessageError {
		/// The message's hash.
		message_hash: XcmHash,
		/// The message's ID (which could also be its hash if nothing overrides it).
		message_id: XcmHash,
		/// Weight required for the message to be executed.
		required_weight: Weight,
	}

	impl<T: Config> Pallet<T> {
		/// Service the message queue up to some given weight `limit`.
		///
		/// Returns the weight consumed by executing messages in the queue.
		fn service_queue(limit: Weight) -> Weight {
			let mut messages_processed = 0;
			PageIndex::<T>::mutate(|page_index| {
				Self::do_service_queue(limit, page_index, &mut messages_processed)
			})
		}

		/// Exactly equivalent to `service_queue` but expects a mutable `page_index` to be passed
		/// in and any changes stored.
		fn do_service_queue(
			limit: Weight,
			page_index: &mut PageIndexData,
			messages_processed: &mut u8,
		) -> Weight {
			let mut used = Weight::zero();
			while page_index.begin_used < page_index.end_used {
				let page = Pages::<T>::take(page_index.begin_used);
				for (i, &(sent_at, ref data)) in page.iter().enumerate() {
					if *messages_processed >= MAX_MESSAGES_PER_BLOCK {
						// Exceeded block message limit - put the remaining messages back and bail
						Pages::<T>::insert(page_index.begin_used, &page[i..]);
						return used
					}
					*messages_processed += 1;
					match Self::try_service_message(limit.saturating_sub(used), sent_at, &data[..])
					{
						Ok(w) => used += w,
						Err(..) => {
							// Too much weight needed - put the remaining messages back and bail
							Pages::<T>::insert(page_index.begin_used, &page[i..]);
							return used
						},
					}
				}
				page_index.begin_used += 1;
			}
			if page_index.begin_used == page_index.end_used {
				// Reset if there's no pages left.
				page_index.begin_used = 0;
				page_index.end_used = 0;
			}
			used
		}

		/// Attempt to service an individual message. Will return `Ok` with the execution weight
		/// consumed unless the message was found to need more weight than `limit`.
		///
		/// NOTE: This will return `Ok` in the case of an error decoding, weighing or executing
		/// the message. This is why it's called message "servicing" rather than "execution".
		pub(crate) fn try_service_message(
			limit: Weight,
			_sent_at: RelayBlockNumber,
			mut data: &[u8],
		) -> Result<Weight, ServiceMessageError> {
			let message_hash = sp_io::hashing::blake2_256(data);
			let mut message_id = message_hash;
			let maybe_msg = VersionedXcm::<T::RuntimeCall>::decode_all_with_depth_limit(
				MAX_XCM_DECODE_DEPTH,
				&mut data,
			)
			.map(Xcm::<T::RuntimeCall>::try_from);
			match maybe_msg {
				Err(_) => {
					Self::deposit_event(Event::InvalidFormat { message_hash });
					Ok(Weight::zero())
				},
				Ok(Err(())) => {
					Self::deposit_event(Event::UnsupportedVersion { message_hash });
					Ok(Weight::zero())
				},
				Ok(Ok(x)) => {
					let outcome = T::XcmExecutor::prepare_and_execute(
						Parent,
						x,
						&mut message_id,
						limit,
						Weight::zero(),
					);
					match outcome {
						Outcome::Error(XcmError::WeightLimitReached(required_weight)) =>
							Err(ServiceMessageError { message_hash, message_id, required_weight }),
						outcome => {
							let weight_used = outcome.weight_used();
							Self::deposit_event(Event::ExecutedDownward {
								message_hash,
								message_id,
								outcome,
							});
							Ok(weight_used)
						},
					}
				},
			}
		}
	}

	/// For an incoming downward message, this just adapts an XCM executor and executes DMP messages
	/// immediately up until some `MaxWeight` at which point it errors. Their origin is asserted to
	/// be the `Parent` location.
	impl<T: Config> DmpMessageHandler for Pallet<T> {
		fn handle_dmp_messages(
			iter: impl Iterator<Item = (RelayBlockNumber, Vec<u8>)>,
			limit: Weight,
		) -> Weight {
			let mut messages_processed = 0;
			let mut page_index = PageIndex::<T>::get();
			let config = Configuration::<T>::get();

			// First try to use `max_weight` to service the current queue.
			let mut used = Self::do_service_queue(limit, &mut page_index, &mut messages_processed);

			// Then if the queue is empty, use the weight remaining to service the incoming messages
			// and once we run out of weight, place them in the queue.
			let item_count = iter.size_hint().0;
			let mut maybe_enqueue_page = if page_index.end_used > page_index.begin_used {
				// queue is already non-empty - start a fresh page.
				Some(Vec::with_capacity(item_count))
			} else {
				None
			};

			for (i, (sent_at, data)) in iter.enumerate() {
				if maybe_enqueue_page.is_none() {
					if messages_processed >= MAX_MESSAGES_PER_BLOCK {
						let item_count_left = item_count.saturating_sub(i);
						maybe_enqueue_page = Some(Vec::with_capacity(item_count_left));

						Self::deposit_event(Event::MaxMessagesExhausted {
							message_hash: sp_io::hashing::blake2_256(&data),
						});
					} else {
						// We're not currently enqueuing - try to execute inline.
						let remaining_weight = limit.saturating_sub(used);
						messages_processed += 1;
						match Self::try_service_message(remaining_weight, sent_at, &data[..]) {
							Ok(consumed) => used += consumed,
							Err(ServiceMessageError {
								message_hash,
								message_id,
								required_weight,
							}) =>
							// Too much weight required right now.
							{
								let is_under_limit =
									Overweight::<T>::count() < MAX_OVERWEIGHT_MESSAGES;
								used.saturating_accrue(T::DbWeight::get().reads(1));
								if required_weight.any_gt(config.max_individual) && is_under_limit {
									// overweight - add to overweight queue and continue with
									// message execution.
									let overweight_index = page_index.overweight_count;
									Overweight::<T>::insert(overweight_index, (sent_at, data));
									Self::deposit_event(Event::OverweightEnqueued {
										message_hash,
										message_id,
										overweight_index,
										required_weight,
									});
									page_index.overweight_count += 1;
									// Not needed for control flow, but only to ensure that the
									// compiler understands that we won't attempt to re-use `data`
									// later.
									continue
								} else {
									// not overweight. stop executing inline and enqueue normally
									// from here on.
									let item_count_left = item_count.saturating_sub(i);
									maybe_enqueue_page = Some(Vec::with_capacity(item_count_left));
									Self::deposit_event(Event::WeightExhausted {
										message_hash,
										message_id,
										remaining_weight,
										required_weight,
									});
								}
							},
						}
					}
				}
				// Cannot be an `else` here since the `maybe_enqueue_page` may have changed.
				if let Some(ref mut enqueue_page) = maybe_enqueue_page {
					enqueue_page.push((sent_at, data));
				}
			}

			// Deposit the enqueued page if any and save the index.
			if let Some(enqueue_page) = maybe_enqueue_page {
				Pages::<T>::insert(page_index.end_used, enqueue_page);
				page_index.end_used += 1;
			}
			PageIndex::<T>::put(page_index);

			used
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate as dmp_queue;

	use codec::Encode;
	use cumulus_primitives_core::ParaId;
	use frame_support::{assert_noop, parameter_types, traits::OnIdle};
	use sp_core::H256;
	use sp_runtime::{
		traits::{BlakeTwo256, IdentityLookup},
		BuildStorage,
		DispatchError::BadOrigin,
	};
	use sp_version::RuntimeVersion;
	use std::cell::RefCell;
	use xcm::latest::{MultiLocation, OriginKind};

	type Block = frame_system::mocking::MockBlock<Test>;
	type Xcm = xcm::latest::Xcm<RuntimeCall>;

	frame_support::construct_runtime!(
		pub enum Test
		{
			System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
			DmpQueue: dmp_queue::{Pallet, Call, Storage, Event<T>},
		}
	);

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub Version: RuntimeVersion = RuntimeVersion {
			spec_name: sp_version::create_runtime_str!("test"),
			impl_name: sp_version::create_runtime_str!("system-test"),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: sp_version::create_apis_vec!([]),
			transaction_version: 1,
			state_version: 1,
		};
		pub const ParachainId: ParaId = ParaId::new(200);
		pub const ReservedXcmpWeight: Weight = Weight::zero();
		pub const ReservedDmpWeight: Weight = Weight::zero();
	}

	type AccountId = u64;

	impl frame_system::Config for Test {
		type RuntimeOrigin = RuntimeOrigin;
		type RuntimeCall = RuntimeCall;
		type Nonce = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Block = Block;
		type RuntimeEvent = RuntimeEvent;
		type BlockHashCount = BlockHashCount;
		type BlockLength = ();
		type BlockWeights = ();
		type Version = Version;
		type PalletInfo = PalletInfo;
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type DbWeight = ();
		type BaseCallFilter = frame_support::traits::Everything;
		type SystemWeightInfo = ();
		type SS58Prefix = ();
		type OnSetCode = ();
		type MaxConsumers = frame_support::traits::ConstU32<16>;
	}

	thread_local! {
		pub static TRACE: RefCell<Vec<(Xcm, Outcome)>> = RefCell::new(Vec::new());
	}
	pub fn take_trace() -> Vec<(Xcm, Outcome)> {
		TRACE.with(|q| {
			let q = &mut *q.borrow_mut();
			let r = q.clone();
			q.clear();
			r
		})
	}

	pub struct MockPrepared(Xcm);
	impl PreparedMessage for MockPrepared {
		fn weight_of(&self) -> Weight {
			match ((self.0).0.len(), &(self.0).0.first()) {
				(1, Some(Transact { require_weight_at_most, .. })) => *require_weight_at_most,
				_ => Weight::from_parts(1, 1),
			}
		}
	}

	pub struct MockExec;
	impl ExecuteXcm<RuntimeCall> for MockExec {
		type Prepared = MockPrepared;

		fn prepare(message: Xcm) -> Result<Self::Prepared, Xcm> {
			Ok(MockPrepared(message))
		}

		fn execute(
			_origin: impl Into<MultiLocation>,
			prepared: MockPrepared,
			_id: &mut XcmHash,
			_weight_credit: Weight,
		) -> Outcome {
			let message = prepared.0;
			let o = match (message.0.len(), &message.0.first()) {
				(1, Some(Transact { require_weight_at_most, .. })) =>
					Outcome::Complete(*require_weight_at_most),
				// use 1000 to decide that it's not supported.
				_ => Outcome::Incomplete(Weight::from_parts(1, 1), XcmError::Unimplemented),
			};
			TRACE.with(|q| q.borrow_mut().push((message, o.clone())));
			o
		}

		fn charge_fees(_location: impl Into<MultiLocation>, _fees: MultiAssets) -> XcmResult {
			Err(XcmError::Unimplemented)
		}
	}

	impl Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type XcmExecutor = MockExec;
		type ExecuteOverweightOrigin = frame_system::EnsureRoot<AccountId>;
	}

	pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
	}

	fn enqueue(enqueued: &[Xcm]) {
		if !enqueued.is_empty() {
			let mut index = PageIndex::<Test>::get();
			Pages::<Test>::insert(
				index.end_used,
				enqueued
					.iter()
					.map(|m| (0, VersionedXcm::<RuntimeCall>::from(m.clone()).encode()))
					.collect::<Vec<_>>(),
			);
			index.end_used += 1;
			PageIndex::<Test>::put(index);
		}
	}

	fn handle_messages(incoming: &[Xcm], limit: Weight) -> Weight {
		let iter = incoming
			.iter()
			.map(|m| (0, VersionedXcm::<RuntimeCall>::from(m.clone()).encode()));
		DmpQueue::handle_dmp_messages(iter, limit)
	}

	fn msg(weight: u64) -> Xcm {
		Xcm(vec![Transact {
			origin_kind: OriginKind::Native,
			require_weight_at_most: Weight::from_parts(weight, weight),
			call: Vec::new().into(),
		}])
	}

	fn msg_complete(weight: u64) -> (Xcm, Outcome) {
		(msg(weight), Outcome::Complete(Weight::from_parts(weight, weight)))
	}

	fn pages_queued() -> PageCounter {
		PageIndex::<Test>::get().end_used - PageIndex::<Test>::get().begin_used
	}

	fn queue_is_empty() -> bool {
		pages_queued() == 0
	}

	fn overweights() -> Vec<OverweightIndex> {
		(0..PageIndex::<Test>::get().overweight_count)
			.filter(|i| Overweight::<Test>::contains_key(i))
			.collect::<Vec<_>>()
	}

	#[test]
	fn basic_setup_works() {
		new_test_ext().execute_with(|| {
			let weight_used = handle_messages(&[], Weight::from_parts(1000, 1000));
			assert_eq!(weight_used, Weight::zero());
			assert_eq!(take_trace(), Vec::new());
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn service_inline_complete_works() {
		new_test_ext().execute_with(|| {
			let incoming = vec![msg(1000), msg(1001)];
			let weight_used = handle_messages(&incoming, Weight::from_parts(2500, 2500));
			assert_eq!(weight_used, Weight::from_parts(2001, 2001));
			assert_eq!(take_trace(), vec![msg_complete(1000), msg_complete(1001)]);
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn service_enqueued_works() {
		new_test_ext().execute_with(|| {
			let enqueued = vec![msg(1000), msg(1001), msg(1002)];
			enqueue(&enqueued);
			let weight_used = handle_messages(&[], Weight::from_parts(2500, 2500));
			assert_eq!(weight_used, Weight::from_parts(2001, 2001));
			assert_eq!(take_trace(), vec![msg_complete(1000), msg_complete(1001),]);
		});
	}

	#[test]
	fn enqueue_works() {
		new_test_ext().execute_with(|| {
			let incoming = vec![msg(1000), msg(1001), msg(1002)];
			let weight_used = handle_messages(&incoming, Weight::from_parts(999, 999));
			assert_eq!(weight_used, Weight::zero());
			assert_eq!(
				PageIndex::<Test>::get(),
				PageIndexData { begin_used: 0, end_used: 1, overweight_count: 0 }
			);
			assert_eq!(Pages::<Test>::get(0).len(), 3);
			assert_eq!(take_trace(), vec![]);

			let weight_used = handle_messages(&[], Weight::from_parts(2500, 2500));
			assert_eq!(weight_used, Weight::from_parts(2001, 2001));
			assert_eq!(take_trace(), vec![msg_complete(1000), msg_complete(1001)]);

			let weight_used = handle_messages(&[], Weight::from_parts(2500, 2500));
			assert_eq!(weight_used, Weight::from_parts(1002, 1002));
			assert_eq!(take_trace(), vec![msg_complete(1002)]);
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn service_inline_then_enqueue_works() {
		new_test_ext().execute_with(|| {
			let incoming = vec![msg(1000), msg(1001), msg(1002)];
			let weight_used = handle_messages(&incoming, Weight::from_parts(1500, 1500));
			assert_eq!(weight_used, Weight::from_parts(1000, 1000));
			assert_eq!(pages_queued(), 1);
			assert_eq!(Pages::<Test>::get(0).len(), 2);
			assert_eq!(take_trace(), vec![msg_complete(1000)]);

			let weight_used = handle_messages(&[], Weight::from_parts(2500, 2500));
			assert_eq!(weight_used, Weight::from_parts(2003, 2003));
			assert_eq!(take_trace(), vec![msg_complete(1001), msg_complete(1002),]);
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn service_enqueued_and_inline_works() {
		new_test_ext().execute_with(|| {
			let enqueued = vec![msg(1000), msg(1001)];
			let incoming = vec![msg(1002), msg(1003)];
			enqueue(&enqueued);
			let weight_used = handle_messages(&incoming, Weight::from_parts(5000, 5000));
			assert_eq!(weight_used, Weight::from_parts(4006, 4006));
			assert_eq!(
				take_trace(),
				vec![
					msg_complete(1000),
					msg_complete(1001),
					msg_complete(1002),
					msg_complete(1003),
				]
			);
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn service_enqueued_partially_and_then_enqueue_works() {
		new_test_ext().execute_with(|| {
			let enqueued = vec![msg(1000), msg(10001)];
			let incoming = vec![msg(1002), msg(1003)];
			enqueue(&enqueued);
			let weight_used = handle_messages(&incoming, Weight::from_parts(5000, 5000));
			assert_eq!(weight_used, Weight::from_parts(1000, 1000));
			assert_eq!(take_trace(), vec![msg_complete(1000)]);
			assert_eq!(pages_queued(), 2);

			// 5000 is not enough to process the 10001 blocker, so nothing happens.
			let weight_used = handle_messages(&[], Weight::from_parts(5000, 5000));
			assert_eq!(weight_used, Weight::zero());
			assert_eq!(take_trace(), vec![]);

			// 20000 is now enough to process everything.
			let weight_used = handle_messages(&[], Weight::from_parts(20000, 20000));
			assert_eq!(weight_used, Weight::from_parts(12006, 12006));
			assert_eq!(
				take_trace(),
				vec![msg_complete(10001), msg_complete(1002), msg_complete(1003),]
			);
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn service_enqueued_completely_and_then_enqueue_works() {
		new_test_ext().execute_with(|| {
			let enqueued = vec![msg(1000), msg(1001)];
			let incoming = vec![msg(10002), msg(1003)];
			enqueue(&enqueued);
			let weight_used = handle_messages(&incoming, Weight::from_parts(5000, 5000));
			assert_eq!(weight_used, Weight::from_parts(2001, 2001));
			assert_eq!(take_trace(), vec![msg_complete(1000), msg_complete(1001)]);
			assert_eq!(pages_queued(), 1);

			// 20000 is now enough to process everything.
			let weight_used = handle_messages(&[], Weight::from_parts(20000, 20000));
			assert_eq!(weight_used, Weight::from_parts(11005, 11005));
			assert_eq!(take_trace(), vec![msg_complete(10002), msg_complete(1003),]);
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn service_enqueued_then_inline_then_enqueue_works() {
		new_test_ext().execute_with(|| {
			let enqueued = vec![msg(1000), msg(1001)];
			let incoming = vec![msg(1002), msg(10003)];
			enqueue(&enqueued);
			let weight_used = handle_messages(&incoming, Weight::from_parts(5000, 5000));
			assert_eq!(weight_used, Weight::from_parts(3003, 3003));
			assert_eq!(
				take_trace(),
				vec![msg_complete(1000), msg_complete(1001), msg_complete(1002),]
			);
			assert_eq!(pages_queued(), 1);

			// 20000 is now enough to process everything.
			let weight_used = handle_messages(&[], Weight::from_parts(20000, 20000));
			assert_eq!(weight_used, Weight::from_parts(10003, 10003));
			assert_eq!(take_trace(), vec![msg_complete(10003),]);
			assert!(queue_is_empty());
		});
	}

	#[test]
	fn page_crawling_works() {
		new_test_ext().execute_with(|| {
			let enqueued = vec![msg(1000), msg(1001)];
			enqueue(&enqueued);
			let weight_used = handle_messages(&[msg(1002)], Weight::from_parts(1500, 1500));
			assert_eq!(weight_used, Weight::from_parts(1000, 1000));
			assert_eq!(take_trace(), vec![msg_complete(1000)]);
			assert_eq!(pages_queued(), 2);
			assert_eq!(PageIndex::<Test>::get().begin_used, 0);

			let weight_used = handle_messages(&[msg(1003)], Weight::from_parts(1500, 1500));
			assert_eq!(weight_used, Weight::from_parts(1001, 1001));
			assert_eq!(take_trace(), vec![msg_complete(1001)]);
			assert_eq!(pages_queued(), 2);
			assert_eq!(PageIndex::<Test>::get().begin_used, 1);

			let weight_used = handle_messages(&[msg(1004)], Weight::from_parts(1500, 1500));
			assert_eq!(weight_used, Weight::from_parts(1002, 1002));
			assert_eq!(take_trace(), vec![msg_complete(1002)]);
			assert_eq!(pages_queued(), 2);
			assert_eq!(PageIndex::<Test>::get().begin_used, 2);
		});
	}

	#[test]
	fn overweight_should_not_block_queue() {
		new_test_ext().execute_with(|| {
			// Set the overweight threshold to 9999.
			Configuration::<Test>::put(ConfigData {
				max_individual: Weight::from_parts(9999, 9999),
			});

			let incoming = vec![msg(1000), msg(10001), msg(1002)];
			let weight_used = handle_messages(&incoming, Weight::from_parts(2500, 2500));
			assert_eq!(weight_used, Weight::from_parts(2002, 2002));
			assert!(queue_is_empty());
			assert_eq!(take_trace(), vec![msg_complete(1000), msg_complete(1002),]);

			assert_eq!(overweights(), vec![0]);
		});
	}

	#[test]
	fn overweights_should_be_manually_executable() {
		new_test_ext().execute_with(|| {
			// Set the overweight threshold to 9999.
			Configuration::<Test>::put(ConfigData {
				max_individual: Weight::from_parts(9999, 9999),
			});

			let incoming = vec![msg(10000)];
			let weight_used = handle_messages(&incoming, Weight::from_parts(2500, 2500));
			assert_eq!(weight_used, Weight::zero());
			assert_eq!(take_trace(), vec![]);
			assert_eq!(overweights(), vec![0]);

			assert_noop!(
				DmpQueue::service_overweight(
					RuntimeOrigin::signed(1),
					0,
					Weight::from_parts(20000, 20000)
				),
				BadOrigin
			);
			assert_noop!(
				DmpQueue::service_overweight(
					RuntimeOrigin::root(),
					1,
					Weight::from_parts(20000, 20000)
				),
				Error::<Test>::Unknown
			);
			assert_noop!(
				DmpQueue::service_overweight(
					RuntimeOrigin::root(),
					0,
					Weight::from_parts(9999, 9999)
				),
				Error::<Test>::OverLimit
			);
			assert_eq!(take_trace(), vec![]);

			let base_weight =
				super::Call::<Test>::service_overweight { index: 0, weight_limit: Weight::zero() }
					.get_dispatch_info()
					.weight;
			use frame_support::dispatch::GetDispatchInfo;
			let info = DmpQueue::service_overweight(
				RuntimeOrigin::root(),
				0,
				Weight::from_parts(20000, 20000),
			)
			.unwrap();
			let actual_weight = info.actual_weight.unwrap();
			assert_eq!(actual_weight, base_weight + Weight::from_parts(10000, 10000));
			assert_eq!(take_trace(), vec![msg_complete(10000)]);
			assert!(overweights().is_empty());

			assert_noop!(
				DmpQueue::service_overweight(
					RuntimeOrigin::root(),
					0,
					Weight::from_parts(20000, 20000)
				),
				Error::<Test>::Unknown
			);
		});
	}

	#[test]
	fn on_idle_should_service_queue() {
		new_test_ext().execute_with(|| {
			enqueue(&[msg(1000), msg(1001)]);
			enqueue(&[msg(1002), msg(1003)]);
			enqueue(&[msg(1004), msg(1005)]);

			let weight_used = DmpQueue::on_idle(1, Weight::from_parts(6000, 6000));
			assert_eq!(weight_used, Weight::from_parts(5010, 5010));
			assert_eq!(
				take_trace(),
				vec![
					msg_complete(1000),
					msg_complete(1001),
					msg_complete(1002),
					msg_complete(1003),
					msg_complete(1004),
				]
			);
			assert_eq!(pages_queued(), 1);
		});
	}

	#[test]
	fn handle_max_messages_per_block() {
		new_test_ext().execute_with(|| {
			enqueue(&[msg(1000), msg(1001)]);
			enqueue(&[msg(1002), msg(1003)]);
			enqueue(&[msg(1004), msg(1005)]);

			let incoming =
				(0..MAX_MESSAGES_PER_BLOCK).map(|i| msg(1006 + i as u64)).collect::<Vec<_>>();
			handle_messages(&incoming, Weight::from_parts(25000, 25000));

			assert_eq!(
				take_trace(),
				(0..MAX_MESSAGES_PER_BLOCK)
					.map(|i| msg_complete(1000 + i as u64))
					.collect::<Vec<_>>(),
			);
			assert_eq!(pages_queued(), 1);

			handle_messages(&[], Weight::from_parts(25000, 25000));
			assert_eq!(
				take_trace(),
				(MAX_MESSAGES_PER_BLOCK..MAX_MESSAGES_PER_BLOCK + 6)
					.map(|i| msg_complete(1000 + i as u64))
					.collect::<Vec<_>>(),
			);
		});
	}
}
