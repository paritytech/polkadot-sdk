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

//! # Generalized Message Queue Pallet
//!
//! Provides generalized message queuing and processing capabilities on a per-queue basis for
//! arbitrary use-cases.
//!
//! # Design Goals
//!
//! 1. Minimal assumptions about `Message`s and `MessageOrigin`s. Both should be MEL bounded blobs.
//!  This ensures the generality and reusability of the pallet.
//! 2. Well known and tightly limited pre-dispatch PoV weights, especially for message execution.
//!  This is paramount for the success of the pallet since message execution is done in
//!  `on_initialize` which must _never_ under-estimate its PoV weight. It also needs a frugal PoV
//!  footprint since PoV is scarce and this is (possibly) done in every block. This must also hold
//! in  the presence of unpredictable message size distributions.
//! 3. Usable as XCMP, DMP and UMP message/dispatch queue - possibly through adapter types.
//!
//! # Design
//!
//! The pallet has means to enqueue, store and process messages. This is implemented by having
//! *queues* which store enqueued messages and can be *served* to process said messages. A queue is
//! identified by its origin in the `BookStateFor`. Each message has an origin which defines into
//! which queue it will be stored. Messages are stored by being appended to the last [`Page`] of a
//! book. Each book keeps track of its pages by indexing `Pages`. The `ReadyRing` contains all
//! queues which hold at least one unprocessed message and are thereby *ready* to be serviced. The
//! `ServiceHead` indicates which *ready* queue is the next to be serviced.
//! The pallet implements [`frame_support::traits::EnqueueMessage`],
//! [`frame_support::traits::ServiceQueues`] and has [`frame_support::traits::ProcessMessage`] and
//! [`OnQueueChanged`] hooks to communicate with the outside world.
//!
//! NOTE: The storage items are not linked since they are not public.
//!
//! **Message Execution**
//!
//! Executing a message is offloaded to the [`Config::MessageProcessor`] which contains the actual
//! logic of how to handle the message since they are blobs. Storage changes are not rolled back on
//! error.
//!
//! A failed message can be temporarily or permanently overweight. The pallet will perpetually try
//! to execute a temporarily overweight message. A permanently overweight message is skipped and
//! must be executed manually.
//!
//! **Reentrancy**
//!
//! This pallet has two entry points for executing (possibly recursive) logic;
//! [`Pallet::service_queues`] and [`Pallet::execute_overweight`]. Both entry points are guarded by
//! the same mutex to error on reentrancy. The only functions that are explicitly **allowed** to be
//! called by a message processor are: [`Pallet::enqueue_message`] and
//! [`Pallet::enqueue_messages`]. All other functions are forbidden and error with
//! [`Error::RecursiveDisallowed`].
//!
//! **Pagination**
//!
//! Queues are stored in a *paged* manner by splitting their messages into [`Page`]s. This results
//! in a lot of complexity when implementing the pallet but is completely necessary to achieve the
//! second #[Design Goal](design-goals). The problem comes from the fact a message can *possibly* be
//! quite large, lets say 64KiB. This then results in a *MEL* of at least 64KiB which results in a
//! PoV of at least 64KiB. Now we have the assumption that most messages are much shorter than their
//! maximum allowed length. This would result in most messages having a pre-dispatch PoV size which
//! is much larger than their post-dispatch PoV size, possibly by a factor of thousand. Disregarding
//! this observation would cripple the processing power of the pallet since it cannot straighten out
//! this discrepancy at runtime. Conceptually, the implementation is packing as many messages into a
//! single bounded vec, as actually fit into the bounds. This reduces the wasted PoV.
//!
//! **Page Data Layout**
//!
//! A Page contains a heap which holds all its messages. The heap is built by concatenating
//! `(ItemHeader, Message)` pairs. The [`ItemHeader`] contains the length of the message which is
//! needed for retrieving it. This layout allows for constant access time of the next message and
//! linear access time for any message in the page. The header must remain minimal to reduce its PoV
//! impact.
//!
//! **Weight Metering**
//!
//! The pallet utilizes the [`sp_weights::WeightMeter`] to manually track its consumption to always
//! stay within the required limit. This implies that the message processor hook can calculate the
//! weight of a message without executing it. This restricts the possible use-cases but is necessary
//! since the pallet runs in `on_initialize` which has a hard weight limit. The weight meter is used
//! in a way that `can_accrue` and `check_accrue` are always used to check the remaining weight of
//! an operation before committing to it. The process of exiting due to insufficient weight is
//! termed "bailing".
//!
//! # Scenario: Message enqueuing
//!
//! A message `m` is enqueued for origin `o` into queue `Q[o]` through
//! [`frame_support::traits::EnqueueMessage::enqueue_message`]`(m, o)`.
//!
//! First the queue is either loaded if it exists or otherwise created with empty default values.
//! The message is then inserted to the queue by appended it into its last `Page` or by creating a
//! new `Page` just for `m` if it does not fit in there. The number of messages in the `Book` is
//! incremented.
//!
//! `Q[o]` is now *ready* which will eventually result in `m` being processed.
//!
//! # Scenario: Message processing
//!
//! The pallet runs each block in `on_initialize` or when being manually called through
//! [`frame_support::traits::ServiceQueues::service_queues`].
//!
//! First it tries to "rotate" the `ReadyRing` by one through advancing the `ServiceHead` to the
//! next *ready* queue. It then starts to service this queue by servicing as many pages of it as
//! possible. Servicing a page means to execute as many message of it as possible. Each executed
//! message is marked as *processed* if the [`Config::MessageProcessor`] return Ok. An event
//! [`Event::Processed`] is emitted afterwards. It is possible that the weight limit of the pallet
//! will never allow a specific message to be executed. In this case it remains as unprocessed and
//! is skipped. This process stops if either there are no more messages in the queue or the
//! remaining weight became insufficient to service this queue. If there is enough weight it tries
//! to advance to the next *ready* queue and service it. This continues until there are no more
//! queues on which it can make progress or not enough weight to check that.
//!
//! # Scenario: Overweight execution
//!
//! A permanently over-weight message which was skipped by the message processing will never be
//! executed automatically through `on_initialize` nor by calling
//! [`frame_support::traits::ServiceQueues::service_queues`].
//!
//! Manual intervention in the form of
//! [`frame_support::traits::ServiceQueues::execute_overweight`] is necessary. Overweight messages
//! emit an [`Event::OverweightEnqueued`] event which can be used to extract the arguments for
//! manual execution. This only works on permanently overweight messages. There is no guarantee that
//! this will work since the message could be part of a stale page and be reaped before execution
//! commences.
//!
//! # Terminology
//!
//! - `Message`: A blob of data into which the pallet has no introspection, defined as
//! [`BoundedSlice<u8, MaxMessageLenOf<T>>`]. The message length is limited by [`MaxMessageLenOf`]
//! which is calculated from [`Config::HeapSize`] and [`ItemHeader::max_encoded_len()`].
//! - `MessageOrigin`: A generic *origin* of a message, defined as [`MessageOriginOf`]. The
//! requirements for it are kept minimal to remain as generic as possible. The type is defined in
//! [`frame_support::traits::ProcessMessage::Origin`].
//! - `Page`: An array of `Message`s, see [`Page`]. Can never be empty.
//! - `Book`: A list of `Page`s, see [`BookState`]. Can be empty.
//! - `Queue`: A `Book` together with an `MessageOrigin` which can be part of the `ReadyRing`. Can
//!   be empty.
//! - `ReadyRing`: A double-linked list which contains all *ready* `Queue`s. It chains together the
//!   queues via their `ready_neighbours` fields. A `Queue` is *ready* if it contains at least one
//!   `Message` which can be processed. Can be empty.
//! - `ServiceHead`: A pointer into the `ReadyRing` to the next `Queue` to be serviced.
//! - (`un`)`processed`: A message is marked as *processed* after it was executed by the pallet. A
//!   message which was either: not yet executed or could not be executed remains as `unprocessed`
//!   which is the default state for a message after being enqueued.
//! - `knitting`/`unknitting`: The means of adding or removing a `Queue` from the `ReadyRing`.
//! - `MEL`: The Max Encoded Length of a type, see [`codec::MaxEncodedLen`].
//! - `Reentrance`: To enter an execution context again before it has completed.
//!
//! # Properties
//!
//! **Liveness - Enqueueing**
//!
//! It is always possible to enqueue any message for any `MessageOrigin`.
//!
//! **Liveness - Processing**
//!
//! `on_initialize` always respects its finite weight-limit.
//!
//! **Progress - Enqueueing**
//!
//! An enqueued message immediately becomes *unprocessed* and thereby eligible for execution.
//!
//! **Progress - Processing**
//!
//! The pallet will execute at least one unprocessed message per block, if there is any. Ensuring
//! this property needs careful consideration of the concrete weights, since it is possible that the
//! weight limit of `on_initialize` never allows for the execution of even one message; trivially if
//! the limit is set to zero. `integrity_test` can be used to ensure that this property holds.
//!
//! **Fairness - Enqueuing**
//!
//! Enqueueing a message for a specific `MessageOrigin` does not influence the ability to enqueue a
//! message for the same of any other `MessageOrigin`; guaranteed by **Liveness - Enqueueing**.
//!
//! **Fairness - Processing**
//!
//! The average amount of weight available for message processing is the same for each queue if the
//! number of queues is constant. Creating a new queue must therefore be, possibly economically,
//! expensive. Currently this is archived by having one queue per para-chain/thread, which keeps the
//! number of queues within `O(n)` and should be "good enough".

#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
mod integration_test;
mod mock;
pub mod mock_helpers;
mod tests;
pub mod weights;

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::{Codec, Decode, Encode, MaxEncodedLen};
use core::{fmt::Debug, ops::Deref};
use frame_support::{
	defensive,
	pallet_prelude::*,
	traits::{
		Defensive, DefensiveSaturating, DefensiveTruncateFrom, EnqueueMessage,
		ExecuteOverweightError, Footprint, ProcessMessage, ProcessMessageError, QueueFootprint,
		QueuePausedQuery, ServiceQueues,
	},
	BoundedSlice, CloneNoBound, DefaultNoBound,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::{defer, H256};
use sp_runtime::{
	traits::{One, Zero},
	SaturatedConversion, Saturating, TransactionOutcome,
};
use sp_weights::WeightMeter;
pub use weights::WeightInfo;

/// Type for identifying a page.
type PageIndex = u32;

/// Data encoded and prefixed to the encoded `MessageItem`.
#[derive(Encode, Decode, PartialEq, MaxEncodedLen, Debug)]
pub struct ItemHeader<Size> {
	/// The length of this item, not including the size of this header. The next item of the page
	/// follows immediately after the payload of this item.
	payload_len: Size,
	/// Whether this item has been processed.
	is_processed: bool,
}

/// A page of messages. Pages always contain at least one item.
#[derive(
	CloneNoBound, Encode, Decode, RuntimeDebugNoBound, DefaultNoBound, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(HeapSize))]
#[codec(mel_bound(Size: MaxEncodedLen))]
pub struct Page<Size: Into<u32> + Debug + Clone + Default, HeapSize: Get<Size>> {
	/// Messages remaining to be processed; this includes overweight messages which have been
	/// skipped.
	remaining: Size,
	/// The size of all remaining messages to be processed.
	///
	/// Includes overweight messages outside of the `first` to `last` window.
	remaining_size: Size,
	/// The number of items before the `first` item in this page.
	first_index: Size,
	/// The heap-offset of the header of the first message item in this page which is ready for
	/// processing.
	first: Size,
	/// The heap-offset of the header of the last message item in this page.
	last: Size,
	/// The heap. If `self.offset == self.heap.len()` then the page is empty and should be deleted.
	heap: BoundedVec<u8, IntoU32<HeapSize, Size>>,
}

impl<
		Size: BaseArithmetic + Unsigned + Copy + Into<u32> + Codec + MaxEncodedLen + Debug + Default,
		HeapSize: Get<Size>,
	> Page<Size, HeapSize>
{
	/// Create a [`Page`] from one unprocessed message.
	fn from_message<T: Config>(message: BoundedSlice<u8, MaxMessageLenOf<T>>) -> Self {
		let payload_len = message.len();
		let data_len = ItemHeader::<Size>::max_encoded_len().saturating_add(payload_len);
		let payload_len = payload_len.saturated_into();
		let header = ItemHeader::<Size> { payload_len, is_processed: false };

		let mut heap = Vec::with_capacity(data_len);
		header.using_encoded(|h| heap.extend_from_slice(h));
		heap.extend_from_slice(message.deref());

		Page {
			remaining: One::one(),
			remaining_size: payload_len,
			first_index: Zero::zero(),
			first: Zero::zero(),
			last: Zero::zero(),
			heap: BoundedVec::defensive_truncate_from(heap),
		}
	}

	/// Try to append one message to a page.
	fn try_append_message<T: Config>(
		&mut self,
		message: BoundedSlice<u8, MaxMessageLenOf<T>>,
	) -> Result<(), ()> {
		let pos = self.heap.len();
		let payload_len = message.len();
		let data_len = ItemHeader::<Size>::max_encoded_len().saturating_add(payload_len);
		let payload_len = payload_len.saturated_into();
		let header = ItemHeader::<Size> { payload_len, is_processed: false };
		let heap_size: u32 = HeapSize::get().into();
		if (heap_size as usize).saturating_sub(self.heap.len()) < data_len {
			// Can't fit.
			return Err(())
		}

		let mut heap = core::mem::take(&mut self.heap).into_inner();
		header.using_encoded(|h| heap.extend_from_slice(h));
		heap.extend_from_slice(message.deref());
		self.heap = BoundedVec::defensive_truncate_from(heap);
		self.last = pos.saturated_into();
		self.remaining.saturating_inc();
		self.remaining_size.saturating_accrue(payload_len);
		Ok(())
	}

	/// Returns the first message in the page without removing it.
	///
	/// SAFETY: Does not panic even on corrupted storage.
	fn peek_first(&self) -> Option<BoundedSlice<u8, IntoU32<HeapSize, Size>>> {
		if self.first > self.last {
			return None
		}
		let f = (self.first.into() as usize).min(self.heap.len());
		let mut item_slice = &self.heap[f..];
		if let Ok(h) = ItemHeader::<Size>::decode(&mut item_slice) {
			let payload_len = h.payload_len.into() as usize;
			if payload_len <= item_slice.len() {
				// impossible to truncate since is sliced up from `self.heap: BoundedVec<u8,
				// HeapSize>`
				return Some(BoundedSlice::defensive_truncate_from(&item_slice[..payload_len]))
			}
		}
		defensive!("message-queue: heap corruption");
		None
	}

	/// Point `first` at the next message, marking the first as processed if `is_processed` is true.
	fn skip_first(&mut self, is_processed: bool) {
		let f = (self.first.into() as usize).min(self.heap.len());
		if let Ok(mut h) = ItemHeader::decode(&mut &self.heap[f..]) {
			if is_processed && !h.is_processed {
				h.is_processed = true;
				h.using_encoded(|d| self.heap[f..f + d.len()].copy_from_slice(d));
				self.remaining.saturating_dec();
				self.remaining_size.saturating_reduce(h.payload_len);
			}
			self.first
				.saturating_accrue(ItemHeader::<Size>::max_encoded_len().saturated_into());
			self.first.saturating_accrue(h.payload_len);
			self.first_index.saturating_inc();
		}
	}

	/// Return the message with index `index` in the form of `(position, processed, message)`.
	fn peek_index(&self, index: usize) -> Option<(usize, bool, &[u8])> {
		let mut pos = 0;
		let mut item_slice = &self.heap[..];
		let header_len: usize = ItemHeader::<Size>::max_encoded_len().saturated_into();
		for _ in 0..index {
			let h = ItemHeader::<Size>::decode(&mut item_slice).ok()?;
			let item_len = h.payload_len.into() as usize;
			if item_slice.len() < item_len {
				return None
			}
			item_slice = &item_slice[item_len..];
			pos.saturating_accrue(header_len.saturating_add(item_len));
		}
		let h = ItemHeader::<Size>::decode(&mut item_slice).ok()?;
		if item_slice.len() < h.payload_len.into() as usize {
			return None
		}
		item_slice = &item_slice[..h.payload_len.into() as usize];
		Some((pos, h.is_processed, item_slice))
	}

	/// Set the `is_processed` flag for the item at `pos` to be `true` if not already and decrement
	/// the `remaining` counter of the page.
	///
	/// Does nothing if no [`ItemHeader`] could be decoded at the given position.
	fn note_processed_at_pos(&mut self, pos: usize) {
		if let Ok(mut h) = ItemHeader::<Size>::decode(&mut &self.heap[pos..]) {
			if !h.is_processed {
				h.is_processed = true;
				h.using_encoded(|d| self.heap[pos..pos + d.len()].copy_from_slice(d));
				self.remaining.saturating_dec();
				self.remaining_size.saturating_reduce(h.payload_len);
			}
		}
	}

	/// Returns whether the page is *complete* which means that no messages remain.
	fn is_complete(&self) -> bool {
		self.remaining.is_zero()
	}
}

/// A single link in the double-linked Ready Ring list.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, PartialEq)]
pub struct Neighbours<MessageOrigin> {
	/// The previous queue.
	prev: MessageOrigin,
	/// The next queue.
	next: MessageOrigin,
}

/// The state of a queue as represented by a book of its pages.
///
/// Each queue has exactly one book which holds all of its pages. All pages of a book combined
/// contain all of the messages of its queue; hence the name *Book*.
/// Books can be chained together in a double-linked fashion through their `ready_neighbours` field.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug)]
pub struct BookState<MessageOrigin> {
	/// The first page with some items to be processed in it. If this is `>= end`, then there are
	/// no pages with items to be processing in them.
	begin: PageIndex,
	/// One more than the last page with some items to be processed in it.
	end: PageIndex,
	/// The number of pages stored at present.
	///
	/// This might be larger than `end-begin`, because we keep pages with unprocessed overweight
	/// messages outside of the end/begin window.
	count: PageIndex,
	/// If this book has any ready pages, then this will be `Some` with the previous and next
	/// neighbours. This wraps around.
	ready_neighbours: Option<Neighbours<MessageOrigin>>,
	/// The number of unprocessed messages stored at present.
	message_count: u64,
	/// The total size of all unprocessed messages stored at present.
	size: u64,
}

impl<MessageOrigin> Default for BookState<MessageOrigin> {
	fn default() -> Self {
		Self { begin: 0, end: 0, count: 0, ready_neighbours: None, message_count: 0, size: 0 }
	}
}

impl<MessageOrigin> From<BookState<MessageOrigin>> for QueueFootprint {
	fn from(book: BookState<MessageOrigin>) -> Self {
		QueueFootprint {
			pages: book.count,
			ready_pages: book.end.defensive_saturating_sub(book.begin),
			storage: Footprint { count: book.message_count, size: book.size },
		}
	}
}

/// Handler code for when the items in a queue change.
pub trait OnQueueChanged<Id> {
	/// Note that the queue `id` now has `item_count` items in it, taking up `items_size` bytes.
	fn on_queue_changed(id: Id, fp: QueueFootprint);
}

impl<Id> OnQueueChanged<Id> for () {
	fn on_queue_changed(_: Id, _: QueueFootprint) {}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Processor for a message.
		///
		/// Storage changes are not rolled back on error.
		///
		/// # Benchmarking
		///
		/// Must be set to [`mock_helpers::NoopMessageProcessor`] for benchmarking.
		/// Other message processors that consumes exactly (1, 1) weight for any give message will
		/// work as well. Otherwise the benchmarking will also measure the weight of the message
		/// processor, which is not desired.
		type MessageProcessor: ProcessMessage;

		/// Page/heap size type.
		type Size: BaseArithmetic
			+ Unsigned
			+ Copy
			+ Into<u32>
			+ Member
			+ Encode
			+ Decode
			+ MaxEncodedLen
			+ TypeInfo
			+ Default;

		/// Code to be called when a message queue changes - either with items introduced or
		/// removed.
		type QueueChangeHandler: OnQueueChanged<<Self::MessageProcessor as ProcessMessage>::Origin>;

		/// Queried by the pallet to check whether a queue can be serviced.
		///
		/// This also applies to manual servicing via `execute_overweight` and `service_queues`. The
		/// value of this is only polled once before servicing the queue. This means that changes to
		/// it that happen *within* the servicing will not be reflected.
		type QueuePausedQuery: QueuePausedQuery<<Self::MessageProcessor as ProcessMessage>::Origin>;

		/// The size of the page; this implies the maximum message size which can be sent.
		///
		/// A good value depends on the expected message sizes, their weights, the weight that is
		/// available for processing them and the maximal needed message size. The maximal message
		/// size is slightly lower than this as defined by [`MaxMessageLenOf`].
		#[pallet::constant]
		type HeapSize: Get<Self::Size>;

		/// The maximum number of stale pages (i.e. of overweight messages) allowed before culling
		/// can happen. Once there are more stale pages than this, then historical pages may be
		/// dropped, even if they contain unprocessed overweight messages.
		#[pallet::constant]
		type MaxStale: Get<u32>;

		/// The amount of weight (if any) which should be provided to the message queue for
		/// servicing enqueued items `on_initialize`.
		///
		/// This may be legitimately `None` in the case that you will call
		/// `ServiceQueues::service_queues` manually or set [`Self::IdleMaxServiceWeight`] to have
		/// it run in `on_idle`.
		#[pallet::constant]
		type ServiceWeight: Get<Option<Weight>>;

		/// The maximum amount of weight (if any) to be used from remaining weight `on_idle` which
		/// should be provided to the message queue for servicing enqueued items `on_idle`.
		/// Useful for parachains to process messages at the same block they are received.
		///
		/// If `None`, it will not call `ServiceQueues::service_queues` in `on_idle`.
		#[pallet::constant]
		type IdleMaxServiceWeight: Get<Option<Weight>>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Message discarded due to an error in the `MessageProcessor` (usually a format error).
		ProcessingFailed {
			/// The `blake2_256` hash of the message.
			id: H256,
			/// The queue of the message.
			origin: MessageOriginOf<T>,
			/// The error that occurred.
			///
			/// This error is pretty opaque. More fine-grained errors need to be emitted as events
			/// by the `MessageProcessor`.
			error: ProcessMessageError,
		},
		/// Message is processed.
		Processed {
			/// The `blake2_256` hash of the message.
			id: H256,
			/// The queue of the message.
			origin: MessageOriginOf<T>,
			/// How much weight was used to process the message.
			weight_used: Weight,
			/// Whether the message was processed.
			///
			/// Note that this does not mean that the underlying `MessageProcessor` was internally
			/// successful. It *solely* means that the MQ pallet will treat this as a success
			/// condition and discard the message. Any internal error needs to be emitted as events
			/// by the `MessageProcessor`.
			success: bool,
		},
		/// Message placed in overweight queue.
		OverweightEnqueued {
			/// The `blake2_256` hash of the message.
			id: [u8; 32],
			/// The queue of the message.
			origin: MessageOriginOf<T>,
			/// The page of the message.
			page_index: PageIndex,
			/// The index of the message within the page.
			message_index: T::Size,
		},
		/// This page was reaped.
		PageReaped {
			/// The queue of the page.
			origin: MessageOriginOf<T>,
			/// The index of the page.
			index: PageIndex,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Page is not reapable because it has items remaining to be processed and is not old
		/// enough.
		NotReapable,
		/// Page to be reaped does not exist.
		NoPage,
		/// The referenced message could not be found.
		NoMessage,
		/// The message was already processed and cannot be processed again.
		AlreadyProcessed,
		/// The message is queued for future execution.
		Queued,
		/// There is temporarily not enough weight to continue servicing messages.
		InsufficientWeight,
		/// This message is temporarily unprocessable.
		///
		/// Such errors are expected, but not guaranteed, to resolve themselves eventually through
		/// retrying.
		TemporarilyUnprocessable,
		/// The queue is paused and no message can be executed from it.
		///
		/// This can change at any time and may resolve in the future by re-trying.
		QueuePaused,
		/// Another call is in progress and needs to finish before this call can happen.
		RecursiveDisallowed,
	}

	/// The index of the first and last (non-empty) pages.
	#[pallet::storage]
	pub(super) type BookStateFor<T: Config> =
		StorageMap<_, Twox64Concat, MessageOriginOf<T>, BookState<MessageOriginOf<T>>, ValueQuery>;

	/// The origin at which we should begin servicing.
	#[pallet::storage]
	pub(super) type ServiceHead<T: Config> = StorageValue<_, MessageOriginOf<T>, OptionQuery>;

	/// The map of page indices to pages.
	#[pallet::storage]
	pub(super) type Pages<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		MessageOriginOf<T>,
		Twox64Concat,
		PageIndex,
		Page<T::Size, T::HeapSize>,
		OptionQuery,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			if let Some(weight_limit) = T::ServiceWeight::get() {
				Self::service_queues_impl(weight_limit, ServiceQueuesContext::OnInitialize)
			} else {
				Weight::zero()
			}
		}

		fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			if let Some(weight_limit) = T::IdleMaxServiceWeight::get() {
				// Make use of the remaining weight to process enqueued messages.
				Self::service_queues_impl(
					weight_limit.min(remaining_weight),
					ServiceQueuesContext::OnIdle,
				)
			} else {
				Weight::zero()
			}
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}

		/// Check all compile-time assumptions about [`crate::Config`].
		#[cfg(test)]
		fn integrity_test() {
			Self::do_integrity_test().expect("Pallet config is valid; qed")
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Remove a page which has no more messages remaining to be processed or is stale.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::reap_page())]
		pub fn reap_page(
			origin: OriginFor<T>,
			message_origin: MessageOriginOf<T>,
			page_index: PageIndex,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			Self::do_reap_page(&message_origin, page_index)
		}

		/// Execute an overweight message.
		///
		/// Temporary processing errors will be propagated whereas permanent errors are treated
		/// as success condition.
		///
		/// - `origin`: Must be `Signed`.
		/// - `message_origin`: The origin from which the message to be executed arrived.
		/// - `page`: The page in the queue in which the message to be executed is sitting.
		/// - `index`: The index into the queue of the message to be executed.
		/// - `weight_limit`: The maximum amount of weight allowed to be consumed in the execution
		///   of the message.
		///
		/// Benchmark complexity considerations: O(index + weight_limit).
		#[pallet::call_index(1)]
		#[pallet::weight(
			T::WeightInfo::execute_overweight_page_updated().max(
			T::WeightInfo::execute_overweight_page_removed()).saturating_add(*weight_limit)
		)]
		pub fn execute_overweight(
			origin: OriginFor<T>,
			message_origin: MessageOriginOf<T>,
			page: PageIndex,
			index: T::Size,
			weight_limit: Weight,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;
			let actual_weight =
				Self::do_execute_overweight(message_origin, page, index, weight_limit)?;
			Ok(Some(actual_weight).into())
		}
	}
}

/// The status of a page after trying to execute its next message.
#[derive(PartialEq, Debug)]
enum PageExecutionStatus {
	/// The execution bailed because there was not enough weight remaining.
	Bailed,
	/// The page did not make any progress on its execution.
	///
	/// This is a transient condition and can be handled by retrying - exactly like [Bailed].
	NoProgress,
	/// No more messages could be loaded. This does _not_ imply `page.is_complete()`.
	///
	/// The reasons for this status are:
	///  - The end of the page is reached but there could still be skipped messages.
	///  - The storage is corrupted.
	NoMore,
}

/// The status after trying to execute the next item of a [`Page`].
#[derive(PartialEq, Debug)]
enum ItemExecutionStatus {
	/// The execution bailed because there was not enough weight remaining.
	Bailed,
	/// The item did not make any progress on its execution.
	///
	/// This is a transient condition and can be handled by retrying - exactly like [Bailed].
	NoProgress,
	/// The item was not found.
	NoItem,
	/// Whether the execution of an item resulted in it being processed.
	///
	/// One reason for `false` would be permanently overweight.
	Executed(bool),
}

/// The status of an attempt to process a message.
#[derive(PartialEq)]
enum MessageExecutionStatus {
	/// There is not enough weight remaining at present.
	InsufficientWeight,
	/// There will never be enough weight.
	Overweight,
	/// The message was processed successfully.
	Processed,
	/// The message was processed and resulted in a, possibly permanent, error.
	Unprocessable { permanent: bool },
	/// The stack depth limit was reached.
	///
	/// We cannot just return `Unprocessable` in this case, because the processability of the
	/// message depends on how the function was called. This may be a permanent error if it was
	/// called by a top-level function, or a transient error if it was already called in a nested
	/// function.
	StackLimitReached,
}

/// The context to pass to [`Pallet::service_queues_impl`] through on_idle and on_initialize hooks
/// We don't want to throw the defensive message if called from on_idle hook
#[derive(PartialEq)]
enum ServiceQueuesContext {
	/// Context of on_idle hook.
	OnIdle,
	/// Context of on_initialize hook.
	OnInitialize,
	/// Context `service_queues` trait function.
	ServiceQueues,
}

impl<T: Config> Pallet<T> {
	/// Knit `origin` into the ready ring right at the end.
	///
	/// Return the two ready ring neighbours of `origin`.
	fn ready_ring_knit(origin: &MessageOriginOf<T>) -> Result<Neighbours<MessageOriginOf<T>>, ()> {
		if let Some(head) = ServiceHead::<T>::get() {
			let mut head_book_state = BookStateFor::<T>::get(&head);
			let mut head_neighbours = head_book_state.ready_neighbours.take().ok_or(())?;
			let tail = head_neighbours.prev;
			head_neighbours.prev = origin.clone();
			head_book_state.ready_neighbours = Some(head_neighbours);
			BookStateFor::<T>::insert(&head, head_book_state);

			let mut tail_book_state = BookStateFor::<T>::get(&tail);
			let mut tail_neighbours = tail_book_state.ready_neighbours.take().ok_or(())?;
			tail_neighbours.next = origin.clone();
			tail_book_state.ready_neighbours = Some(tail_neighbours);
			BookStateFor::<T>::insert(&tail, tail_book_state);

			Ok(Neighbours { next: head, prev: tail })
		} else {
			ServiceHead::<T>::put(origin);
			Ok(Neighbours { next: origin.clone(), prev: origin.clone() })
		}
	}

	fn ready_ring_unknit(origin: &MessageOriginOf<T>, neighbours: Neighbours<MessageOriginOf<T>>) {
		if origin == &neighbours.next {
			debug_assert!(
				origin == &neighbours.prev,
				"unknitting from single item ring; outgoing must be only item"
			);
			// Service queue empty.
			ServiceHead::<T>::kill();
		} else {
			BookStateFor::<T>::mutate(&neighbours.next, |book_state| {
				if let Some(ref mut n) = book_state.ready_neighbours {
					n.prev = neighbours.prev.clone()
				}
			});
			BookStateFor::<T>::mutate(&neighbours.prev, |book_state| {
				if let Some(ref mut n) = book_state.ready_neighbours {
					n.next = neighbours.next.clone()
				}
			});
			if let Some(head) = ServiceHead::<T>::get() {
				if &head == origin {
					ServiceHead::<T>::put(neighbours.next);
				}
			} else {
				defensive!("`ServiceHead` must be some if there was a ready queue");
			}
		}
	}

	/// Tries to bump the current `ServiceHead` to the next ready queue.
	///
	/// Returns the current head if it got be bumped and `None` otherwise.
	fn bump_service_head(weight: &mut WeightMeter) -> Option<MessageOriginOf<T>> {
		if weight.try_consume(T::WeightInfo::bump_service_head()).is_err() {
			return None
		}

		if let Some(head) = ServiceHead::<T>::get() {
			let mut head_book_state = BookStateFor::<T>::get(&head);
			if let Some(head_neighbours) = head_book_state.ready_neighbours.take() {
				ServiceHead::<T>::put(&head_neighbours.next);
				Some(head)
			} else {
				None
			}
		} else {
			None
		}
	}

	/// The maximal weight that a single message can consume.
	///
	/// Any message using more than this will be marked as permanently overweight and not
	/// automatically re-attempted. Returns `None` if the servicing of a message cannot begin.
	/// `Some(0)` means that only messages with no weight may be served.
	fn max_message_weight(limit: Weight) -> Option<Weight> {
		limit.checked_sub(&Self::single_msg_overhead())
	}

	/// The overhead of servicing a single message.
	fn single_msg_overhead() -> Weight {
		T::WeightInfo::bump_service_head()
			.saturating_add(T::WeightInfo::service_queue_base())
			.saturating_add(
				T::WeightInfo::service_page_base_completion()
					.max(T::WeightInfo::service_page_base_no_completion()),
			)
			.saturating_add(T::WeightInfo::service_page_item())
			.saturating_add(T::WeightInfo::ready_ring_unknit())
	}

	/// Checks invariants of the pallet config.
	///
	/// The results of this can only be relied upon if the config values are set to constants.
	#[cfg(test)]
	fn do_integrity_test() -> Result<(), String> {
		ensure!(!MaxMessageLenOf::<T>::get().is_zero(), "HeapSize too low");

		if let Some(service) = T::ServiceWeight::get() {
			if Self::max_message_weight(service).is_none() {
				return Err(format!(
					"ServiceWeight too low: {}. Must be at least {}",
					service,
					Self::single_msg_overhead(),
				))
			}
		}

		Ok(())
	}

	fn do_enqueue_message(
		origin: &MessageOriginOf<T>,
		message: BoundedSlice<u8, MaxMessageLenOf<T>>,
	) {
		let mut book_state = BookStateFor::<T>::get(origin);
		book_state.message_count.saturating_inc();
		book_state
			.size
			// This should be payload size, but here the payload *is* the message.
			.saturating_accrue(message.len() as u64);

		if book_state.end > book_state.begin {
			debug_assert!(book_state.ready_neighbours.is_some(), "Must be in ready ring if ready");
			// Already have a page in progress - attempt to append.
			let last = book_state.end - 1;
			let mut page = match Pages::<T>::get(origin, last) {
				Some(p) => p,
				None => {
					defensive!("Corruption: referenced page doesn't exist.");
					return
				},
			};
			if page.try_append_message::<T>(message).is_ok() {
				Pages::<T>::insert(origin, last, &page);
				BookStateFor::<T>::insert(origin, book_state);
				return
			}
		} else {
			debug_assert!(
				book_state.ready_neighbours.is_none(),
				"Must not be in ready ring if not ready"
			);
			// insert into ready queue.
			match Self::ready_ring_knit(origin) {
				Ok(neighbours) => book_state.ready_neighbours = Some(neighbours),
				Err(()) => {
					defensive!("Ring state invalid when knitting");
				},
			}
		}
		// No room on the page or no page - link in a new page.
		book_state.end.saturating_inc();
		book_state.count.saturating_inc();
		let page = Page::from_message::<T>(message);
		Pages::<T>::insert(origin, book_state.end - 1, page);
		// NOTE: `T::QueueChangeHandler` is called by the caller.
		BookStateFor::<T>::insert(origin, book_state);
	}

	/// Try to execute a single message that was marked as overweight.
	///
	/// The `weight_limit` is the weight that can be consumed to execute the message. The base
	/// weight of the function it self must be measured by the caller.
	pub fn do_execute_overweight(
		origin: MessageOriginOf<T>,
		page_index: PageIndex,
		index: T::Size,
		weight_limit: Weight,
	) -> Result<Weight, Error<T>> {
		match with_service_mutex(|| {
			Self::do_execute_overweight_inner(origin, page_index, index, weight_limit)
		}) {
			Err(()) => Err(Error::<T>::RecursiveDisallowed),
			Ok(x) => x,
		}
	}

	/// Same as `do_execute_overweight` but must be called while holding the `service_mutex`.
	fn do_execute_overweight_inner(
		origin: MessageOriginOf<T>,
		page_index: PageIndex,
		index: T::Size,
		weight_limit: Weight,
	) -> Result<Weight, Error<T>> {
		let mut book_state = BookStateFor::<T>::get(&origin);
		ensure!(!T::QueuePausedQuery::is_paused(&origin), Error::<T>::QueuePaused);

		let mut page = Pages::<T>::get(&origin, page_index).ok_or(Error::<T>::NoPage)?;
		let (pos, is_processed, payload) =
			page.peek_index(index.into() as usize).ok_or(Error::<T>::NoMessage)?;
		let payload_len = payload.len() as u64;
		ensure!(
			page_index < book_state.begin ||
				(page_index == book_state.begin && pos < page.first.into() as usize),
			Error::<T>::Queued
		);
		ensure!(!is_processed, Error::<T>::AlreadyProcessed);
		use MessageExecutionStatus::*;
		let mut weight_counter = WeightMeter::with_limit(weight_limit);
		match Self::process_message_payload(
			origin.clone(),
			page_index,
			index,
			payload,
			&mut weight_counter,
			Weight::MAX,
			// ^^^ We never recognise it as permanently overweight, since that would result in an
			// additional overweight event being deposited.
		) {
			Overweight | InsufficientWeight => Err(Error::<T>::InsufficientWeight),
			StackLimitReached | Unprocessable { permanent: false } =>
				Err(Error::<T>::TemporarilyUnprocessable),
			Unprocessable { permanent: true } | Processed => {
				page.note_processed_at_pos(pos);
				book_state.message_count.saturating_dec();
				book_state.size.saturating_reduce(payload_len);
				let page_weight = if page.remaining.is_zero() {
					debug_assert!(
						page.remaining_size.is_zero(),
						"no messages remaining; no space taken; qed"
					);
					Pages::<T>::remove(&origin, page_index);
					debug_assert!(book_state.count >= 1, "page exists, so book must have pages");
					book_state.count.saturating_dec();
					T::WeightInfo::execute_overweight_page_removed()
				// no need to consider .first or ready ring since processing an overweight page
				// would not alter that state.
				} else {
					Pages::<T>::insert(&origin, page_index, page);
					T::WeightInfo::execute_overweight_page_updated()
				};
				BookStateFor::<T>::insert(&origin, &book_state);
				T::QueueChangeHandler::on_queue_changed(origin, book_state.into());
				Ok(weight_counter.consumed().saturating_add(page_weight))
			},
		}
	}

	/// Remove a stale page or one which has no more messages remaining to be processed.
	fn do_reap_page(origin: &MessageOriginOf<T>, page_index: PageIndex) -> DispatchResult {
		match with_service_mutex(|| Self::do_reap_page_inner(origin, page_index)) {
			Err(()) => Err(Error::<T>::RecursiveDisallowed.into()),
			Ok(x) => x,
		}
	}

	/// Same as `do_reap_page` but must be called while holding the `service_mutex`.
	fn do_reap_page_inner(origin: &MessageOriginOf<T>, page_index: PageIndex) -> DispatchResult {
		let mut book_state = BookStateFor::<T>::get(origin);
		// definitely not reapable if the page's index is no less than the `begin`ning of ready
		// pages.
		ensure!(page_index < book_state.begin, Error::<T>::NotReapable);

		let page = Pages::<T>::get(origin, page_index).ok_or(Error::<T>::NoPage)?;

		// definitely reapable if the page has no messages in it.
		let reapable = page.remaining.is_zero();

		// also reapable if the page index has dropped below our watermark.
		let cullable = || {
			let total_pages = book_state.count;
			let ready_pages = book_state.end.saturating_sub(book_state.begin).min(total_pages);

			// The number of stale pages - i.e. pages which contain unprocessed overweight messages.
			// We would prefer to keep these around but will restrict how far into history they can
			// extend if we notice that there's too many of them.
			//
			// We don't know *where* in history these pages are so we use a dynamic formula which
			// reduces the historical time horizon as the stale pages pile up and increases it as
			// they reduce.
			let stale_pages = total_pages - ready_pages;

			// The maximum number of stale pages (i.e. of overweight messages) allowed before
			// culling can happen at all. Once there are more stale pages than this, then historical
			// pages may be dropped, even if they contain unprocessed overweight messages.
			let max_stale = T::MaxStale::get();

			// The amount beyond the maximum which are being used. If it's not beyond the maximum
			// then we exit now since no culling is needed.
			let overflow = match stale_pages.checked_sub(max_stale + 1) {
				Some(x) => x + 1,
				None => return false,
			};

			// The special formula which tells us how deep into index-history we will pages. As
			// the overflow is greater (and thus the need to drop items from storage is more urgent)
			// this is reduced, allowing a greater range of pages to be culled.
			// With a minimum `overflow` (`1`), this returns `max_stale ** 2`, indicating we only
			// cull beyond that number of indices deep into history.
			// At this overflow increases, our depth reduces down to a limit of `max_stale`. We
			// never want to reduce below this since this will certainly allow enough pages to be
			// culled in order to bring `overflow` back to zero.
			let backlog = (max_stale * max_stale / overflow).max(max_stale);

			let watermark = book_state.begin.saturating_sub(backlog);
			page_index < watermark
		};
		ensure!(reapable || cullable(), Error::<T>::NotReapable);

		Pages::<T>::remove(origin, page_index);
		debug_assert!(book_state.count > 0, "reaping a page implies there are pages");
		book_state.count.saturating_dec();
		book_state.message_count.saturating_reduce(page.remaining.into() as u64);
		book_state.size.saturating_reduce(page.remaining_size.into() as u64);
		BookStateFor::<T>::insert(origin, &book_state);
		T::QueueChangeHandler::on_queue_changed(origin.clone(), book_state.into());
		Self::deposit_event(Event::PageReaped { origin: origin.clone(), index: page_index });

		Ok(())
	}

	/// Execute any messages remaining to be processed in the queue of `origin`, using up to
	/// `weight_limit` to do so. Any messages which would take more than `overweight_limit` to
	/// execute are deemed overweight and ignored.
	fn service_queue(
		origin: MessageOriginOf<T>,
		weight: &mut WeightMeter,
		overweight_limit: Weight,
	) -> (bool, Option<MessageOriginOf<T>>) {
		use PageExecutionStatus::*;
		if weight
			.try_consume(
				T::WeightInfo::service_queue_base()
					.saturating_add(T::WeightInfo::ready_ring_unknit()),
			)
			.is_err()
		{
			return (false, None)
		}

		let mut book_state = BookStateFor::<T>::get(&origin);
		let mut total_processed = 0;
		if T::QueuePausedQuery::is_paused(&origin) {
			let next_ready = book_state.ready_neighbours.as_ref().map(|x| x.next.clone());
			return (false, next_ready)
		}

		while book_state.end > book_state.begin {
			let (processed, status) =
				Self::service_page(&origin, &mut book_state, weight, overweight_limit);
			total_processed.saturating_accrue(processed);
			match status {
				// Store the page progress and do not go to the next one.
				Bailed | NoProgress => break,
				// Go to the next page if this one is at the end.
				NoMore => (),
			};
			book_state.begin.saturating_inc();
		}
		let next_ready = book_state.ready_neighbours.as_ref().map(|x| x.next.clone());
		if book_state.begin >= book_state.end {
			// No longer ready - unknit.
			if let Some(neighbours) = book_state.ready_neighbours.take() {
				Self::ready_ring_unknit(&origin, neighbours);
			} else if total_processed > 0 {
				defensive!("Freshly processed queue must have been ready");
			}
		}
		BookStateFor::<T>::insert(&origin, &book_state);
		if total_processed > 0 {
			T::QueueChangeHandler::on_queue_changed(origin, book_state.into());
		}
		(total_processed > 0, next_ready)
	}

	/// Service as many messages of a page as possible.
	///
	/// Returns how many messages were processed and the page's status.
	fn service_page(
		origin: &MessageOriginOf<T>,
		book_state: &mut BookStateOf<T>,
		weight: &mut WeightMeter,
		overweight_limit: Weight,
	) -> (u32, PageExecutionStatus) {
		use PageExecutionStatus::*;
		if weight
			.try_consume(
				T::WeightInfo::service_page_base_completion()
					.max(T::WeightInfo::service_page_base_no_completion()),
			)
			.is_err()
		{
			return (0, Bailed)
		}

		let page_index = book_state.begin;
		let mut page = match Pages::<T>::get(origin, page_index) {
			Some(p) => p,
			None => {
				defensive!("message-queue: referenced page not found");
				return (0, NoMore)
			},
		};

		let mut total_processed = 0;

		// Execute as many messages as possible.
		let status = loop {
			use ItemExecutionStatus::*;
			match Self::service_page_item(
				origin,
				page_index,
				book_state,
				&mut page,
				weight,
				overweight_limit,
			) {
				Bailed => break PageExecutionStatus::Bailed,
				NoItem => break PageExecutionStatus::NoMore,
				NoProgress => break PageExecutionStatus::NoProgress,
				// Keep going as long as we make progress...
				Executed(true) => total_processed.saturating_inc(),
				Executed(false) => (),
			}
		};

		if page.is_complete() {
			debug_assert!(status != Bailed, "we never bail if a page became complete");
			Pages::<T>::remove(origin, page_index);
			debug_assert!(book_state.count > 0, "completing a page implies there are pages");
			book_state.count.saturating_dec();
		} else {
			Pages::<T>::insert(origin, page_index, page);
		}
		(total_processed, status)
	}

	/// Execute the next message of a page.
	pub(crate) fn service_page_item(
		origin: &MessageOriginOf<T>,
		page_index: PageIndex,
		book_state: &mut BookStateOf<T>,
		page: &mut PageOf<T>,
		weight: &mut WeightMeter,
		overweight_limit: Weight,
	) -> ItemExecutionStatus {
		use MessageExecutionStatus::*;
		// This ugly pre-checking is needed for the invariant
		// "we never bail if a page became complete".
		if page.is_complete() {
			return ItemExecutionStatus::NoItem
		}
		if weight.try_consume(T::WeightInfo::service_page_item()).is_err() {
			return ItemExecutionStatus::Bailed
		}

		let payload = &match page.peek_first() {
			Some(m) => m,
			None => return ItemExecutionStatus::NoItem,
		}[..];
		let payload_len = payload.len() as u64;

		// Store these for the case that `process_message_payload` is recursive.
		Pages::<T>::insert(origin, page_index, &*page);
		BookStateFor::<T>::insert(origin, &*book_state);

		let res = Self::process_message_payload(
			origin.clone(),
			page_index,
			page.first_index,
			payload,
			weight,
			overweight_limit,
		);

		// And restore them afterwards to see the changes of a recursive call.
		*book_state = BookStateFor::<T>::get(origin);
		if let Some(new_page) = Pages::<T>::get(origin, page_index) {
			*page = new_page;
		} else {
			defensive!("page must exist since we just inserted it and recursive calls are not allowed to remove anything");
			return ItemExecutionStatus::NoItem
		};

		let is_processed = match res {
			InsufficientWeight => return ItemExecutionStatus::Bailed,
			Unprocessable { permanent: false } => return ItemExecutionStatus::NoProgress,
			Processed | Unprocessable { permanent: true } | StackLimitReached => true,
			Overweight => false,
		};

		if is_processed {
			book_state.message_count.saturating_dec();
			book_state.size.saturating_reduce(payload_len as u64);
		}
		page.skip_first(is_processed);
		ItemExecutionStatus::Executed(is_processed)
	}

	/// Ensure the correctness of state of this pallet.
	///
	/// # Assumptions-
	///
	/// If `serviceHead` points to a ready Queue, then BookState of that Queue has:
	///
	/// * `message_count` > 0
	/// * `size` > 0
	/// * `end` > `begin`
	/// * Some(ready_neighbours)
	/// * If `ready_neighbours.next` == self.origin, then `ready_neighbours.prev` == self.origin
	///   (only queue in ring)
	///
	/// For Pages(begin to end-1) in BookState:
	///
	/// * `remaining` > 0
	/// * `remaining_size` > 0
	/// * `first` <= `last`
	/// * Every page can be decoded into peek_* functions
	#[cfg(any(test, feature = "try-runtime", feature = "std"))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		// Checking memory corruption for BookStateFor
		ensure!(
			BookStateFor::<T>::iter_keys().count() == BookStateFor::<T>::iter_values().count(),
			"Memory Corruption in BookStateFor"
		);
		// Checking memory corruption for Pages
		ensure!(
			Pages::<T>::iter_keys().count() == Pages::<T>::iter_values().count(),
			"Memory Corruption in Pages"
		);

		// Basic checks for each book
		for book in BookStateFor::<T>::iter_values() {
			ensure!(book.end >= book.begin, "Invariant");
			ensure!(book.end < 1 << 30, "Likely overflow or corruption");
			ensure!(book.message_count < 1 << 30, "Likely overflow or corruption");
			ensure!(book.size < 1 << 30, "Likely overflow or corruption");
			ensure!(book.count < 1 << 30, "Likely overflow or corruption");

			let fp: QueueFootprint = book.into();
			ensure!(fp.ready_pages <= fp.pages, "There cannot be more ready than total pages");
		}

		//loop around this origin
		let Some(starting_origin) = ServiceHead::<T>::get() else { return Ok(()) };

		while let Some(head) = Self::bump_service_head(&mut WeightMeter::new()) {
			ensure!(
				BookStateFor::<T>::contains_key(&head),
				"Service head must point to an existing book"
			);

			let head_book_state = BookStateFor::<T>::get(&head);
			ensure!(
				head_book_state.message_count > 0,
				"There must be some messages if in ReadyRing"
			);
			ensure!(head_book_state.size > 0, "There must be some message size if in ReadyRing");
			ensure!(
				head_book_state.end > head_book_state.begin,
				"End > Begin if unprocessed messages exists"
			);
			ensure!(
				head_book_state.ready_neighbours.is_some(),
				"There must be neighbours if in ReadyRing"
			);

			if head_book_state.ready_neighbours.as_ref().unwrap().next == head {
				ensure!(
					head_book_state.ready_neighbours.as_ref().unwrap().prev == head,
					"Can only happen if only queue in ReadyRing"
				);
			}

			for page_index in head_book_state.begin..head_book_state.end {
				let page = Pages::<T>::get(&head, page_index).unwrap();
				let remaining_messages = page.remaining;
				let mut counted_remaining_messages: u32 = 0;
				ensure!(
					remaining_messages > 0.into(),
					"These must be some messages that have not been processed yet!"
				);

				for i in 0..u32::MAX {
					if let Some((_, processed, _)) = page.peek_index(i as usize) {
						if !processed {
							counted_remaining_messages += 1;
						}
					} else {
						break
					}
				}

				ensure!(
					remaining_messages.into() == counted_remaining_messages,
					"Memory Corruption"
				);
			}

			if head_book_state.ready_neighbours.as_ref().unwrap().next == starting_origin {
				break
			}
		}
		Ok(())
	}

	/// Print the pages in each queue and the messages in each page.
	///
	/// Processed messages are prefixed with a `*` and the current `begin`ning page with a `>`.
	///
	/// # Example output
	///
	/// ```text
	/// queue Here:
	///   page 0: []
	/// > page 1: []
	///   page 2: ["\0weight=4", "\0c", ]
	///   page 3: ["\0bigbig 1", ]
	///   page 4: ["\0bigbig 2", ]
	///   page 5: ["\0bigbig 3", ]
	/// ```
	#[cfg(feature = "std")]
	pub fn debug_info() -> String {
		let mut info = String::new();
		for (origin, book_state) in BookStateFor::<T>::iter() {
			let mut queue = format!("queue {:?}:\n", &origin);
			let mut pages = Pages::<T>::iter_prefix(&origin).collect::<Vec<_>>();
			pages.sort_by(|(a, _), (b, _)| a.cmp(b));
			for (page_index, mut page) in pages.into_iter() {
				let page_info = if book_state.begin == page_index { ">" } else { " " };
				let mut page_info = format!(
					"{} page {} ({:?} first, {:?} last, {:?} remain): [ ",
					page_info, page_index, page.first, page.last, page.remaining
				);
				for i in 0..u32::MAX {
					if let Some((_, processed, message)) =
						page.peek_index(i.try_into().expect("std-only code"))
					{
						let msg = String::from_utf8_lossy(message);
						if processed {
							page_info.push('*');
						}
						page_info.push_str(&format!("{:?}, ", msg));
						page.skip_first(true);
					} else {
						break
					}
				}
				page_info.push_str("]\n");
				queue.push_str(&page_info);
			}
			info.push_str(&queue);
		}
		info
	}

	/// Process a single message.
	///
	/// The base weight of this function needs to be accounted for by the caller. `weight` is the
	/// remaining weight to process the message. `overweight_limit` is the maximum weight that a
	/// message can ever consume. Messages above this limit are marked as permanently overweight.
	/// This process is also transactional, any form of error that occurs in processing a message
	/// causes storage changes to be rolled back.
	fn process_message_payload(
		origin: MessageOriginOf<T>,
		page_index: PageIndex,
		message_index: T::Size,
		message: &[u8],
		meter: &mut WeightMeter,
		overweight_limit: Weight,
	) -> MessageExecutionStatus {
		let mut id = sp_io::hashing::blake2_256(message);
		use ProcessMessageError::*;
		let prev_consumed = meter.consumed();

		let transaction =
			storage::with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
				let res =
					T::MessageProcessor::process_message(message, origin.clone(), meter, &mut id);
				match &res {
					Ok(_) => TransactionOutcome::Commit(Ok(res)),
					Err(_) => TransactionOutcome::Rollback(Ok(res)),
				}
			});

		let transaction = match transaction {
			Ok(result) => result,
			_ => {
				defensive!(
					"Error occurred processing message, storage changes will be rolled back"
				);
				return MessageExecutionStatus::Unprocessable { permanent: true }
			},
		};

		match transaction {
			Err(Overweight(w)) if w.any_gt(overweight_limit) => {
				// Permanently overweight.
				Self::deposit_event(Event::<T>::OverweightEnqueued {
					id,
					origin,
					page_index,
					message_index,
				});
				MessageExecutionStatus::Overweight
			},
			Err(Overweight(_)) => {
				// Temporarily overweight - save progress and stop processing this
				// queue.
				MessageExecutionStatus::InsufficientWeight
			},
			Err(Yield) => {
				// Processing should be reattempted later.
				MessageExecutionStatus::Unprocessable { permanent: false }
			},
			Err(error @ BadFormat | error @ Corrupt | error @ Unsupported) => {
				// Permanent error - drop
				Self::deposit_event(Event::<T>::ProcessingFailed { id: id.into(), origin, error });
				MessageExecutionStatus::Unprocessable { permanent: true }
			},
			Err(error @ StackLimitReached) => {
				Self::deposit_event(Event::<T>::ProcessingFailed { id: id.into(), origin, error });
				MessageExecutionStatus::StackLimitReached
			},
			Ok(success) => {
				// Success
				let weight_used = meter.consumed().saturating_sub(prev_consumed);
				Self::deposit_event(Event::<T>::Processed {
					id: id.into(),
					origin,
					weight_used,
					success,
				});
				MessageExecutionStatus::Processed
			},
		}
	}

	fn service_queues_impl(weight_limit: Weight, context: ServiceQueuesContext) -> Weight {
		let mut weight = WeightMeter::with_limit(weight_limit);

		// Get the maximum weight that processing a single message may take:
		let max_weight = Self::max_message_weight(weight_limit).unwrap_or_else(|| {
			if matches!(context, ServiceQueuesContext::OnInitialize) {
				defensive!("Not enough weight to service a single message.");
			}
			Weight::zero()
		});

		match with_service_mutex(|| {
			let mut next = match Self::bump_service_head(&mut weight) {
				Some(h) => h,
				None => return weight.consumed(),
			};
			// The last queue that did not make any progress.
			// The loop aborts as soon as it arrives at this queue again without making any progress
			// on other queues in between.
			let mut last_no_progress = None;

			loop {
				let (progressed, n) = Self::service_queue(next.clone(), &mut weight, max_weight);
				next = match n {
					Some(n) =>
						if !progressed {
							if last_no_progress == Some(n.clone()) {
								break
							}
							if last_no_progress.is_none() {
								last_no_progress = Some(next.clone())
							}
							n
						} else {
							last_no_progress = None;
							n
						},
					None => break,
				}
			}
			weight.consumed()
		}) {
			Err(()) => weight.consumed(),
			Ok(w) => w,
		}
	}
}

/// Run a closure that errors on re-entrance. Meant to be used by anything that services queues.
pub(crate) fn with_service_mutex<F: FnOnce() -> R, R>(f: F) -> Result<R, ()> {
	// Holds the singleton token instance.
	environmental::environmental!(token: Option<()>);

	token::using_once(&mut Some(()), || {
		// The first `ok_or` should always be `Ok` since we are inside a `using_once`.
		let hold = token::with(|t| t.take()).ok_or(()).defensive()?.ok_or(())?;

		// Put the token back when we're done.
		defer! {
			token::with(|t| {
				*t = Some(hold);
			});
		}

		Ok(f())
	})
}

/// Provides a [`sp_core::Get`] to access the `MEL` of a [`codec::MaxEncodedLen`] type.
pub struct MaxEncodedLenOf<T>(core::marker::PhantomData<T>);
impl<T: MaxEncodedLen> Get<u32> for MaxEncodedLenOf<T> {
	fn get() -> u32 {
		T::max_encoded_len() as u32
	}
}

/// Calculates the maximum message length and exposed it through the [`codec::MaxEncodedLen`] trait.
pub struct MaxMessageLen<Origin, Size, HeapSize>(
	core::marker::PhantomData<(Origin, Size, HeapSize)>,
);
impl<Origin: MaxEncodedLen, Size: MaxEncodedLen + Into<u32>, HeapSize: Get<Size>> Get<u32>
	for MaxMessageLen<Origin, Size, HeapSize>
{
	fn get() -> u32 {
		(HeapSize::get().into()).saturating_sub(ItemHeader::<Size>::max_encoded_len() as u32)
	}
}

/// The maximal message length.
pub type MaxMessageLenOf<T> =
	MaxMessageLen<MessageOriginOf<T>, <T as Config>::Size, <T as Config>::HeapSize>;
/// The maximal encoded origin length.
pub type MaxOriginLenOf<T> = MaxEncodedLenOf<MessageOriginOf<T>>;
/// The `MessageOrigin` of this pallet.
pub type MessageOriginOf<T> = <<T as Config>::MessageProcessor as ProcessMessage>::Origin;
/// The maximal heap size of a page.
pub type HeapSizeU32Of<T> = IntoU32<<T as Config>::HeapSize, <T as Config>::Size>;
/// The [`Page`] of this pallet.
pub type PageOf<T> = Page<<T as Config>::Size, <T as Config>::HeapSize>;
/// The [`BookState`] of this pallet.
pub type BookStateOf<T> = BookState<MessageOriginOf<T>>;

/// Converts a [`sp_core::Get`] with returns a type that can be cast into an `u32` into a `Get`
/// which returns an `u32`.
pub struct IntoU32<T, O>(core::marker::PhantomData<(T, O)>);
impl<T: Get<O>, O: Into<u32>> Get<u32> for IntoU32<T, O> {
	fn get() -> u32 {
		T::get().into()
	}
}

impl<T: Config> ServiceQueues for Pallet<T> {
	type OverweightMessageAddress = (MessageOriginOf<T>, PageIndex, T::Size);

	fn service_queues(weight_limit: Weight) -> Weight {
		Self::service_queues_impl(weight_limit, ServiceQueuesContext::ServiceQueues)
	}

	/// Execute a single overweight message.
	///
	/// The weight limit must be enough for `execute_overweight` and the message execution itself.
	fn execute_overweight(
		weight_limit: Weight,
		(message_origin, page, index): Self::OverweightMessageAddress,
	) -> Result<Weight, ExecuteOverweightError> {
		let mut weight = WeightMeter::with_limit(weight_limit);
		if weight
			.try_consume(
				T::WeightInfo::execute_overweight_page_removed()
					.max(T::WeightInfo::execute_overweight_page_updated()),
			)
			.is_err()
		{
			return Err(ExecuteOverweightError::InsufficientWeight)
		}

		Pallet::<T>::do_execute_overweight(message_origin, page, index, weight.remaining()).map_err(
			|e| match e {
				Error::<T>::InsufficientWeight => ExecuteOverweightError::InsufficientWeight,
				Error::<T>::AlreadyProcessed => ExecuteOverweightError::AlreadyProcessed,
				Error::<T>::QueuePaused => ExecuteOverweightError::QueuePaused,
				Error::<T>::NoPage | Error::<T>::NoMessage | Error::<T>::Queued =>
					ExecuteOverweightError::NotFound,
				Error::<T>::RecursiveDisallowed => ExecuteOverweightError::RecursiveDisallowed,
				_ => ExecuteOverweightError::Other,
			},
		)
	}
}

impl<T: Config> EnqueueMessage<MessageOriginOf<T>> for Pallet<T> {
	type MaxMessageLen =
		MaxMessageLen<<T::MessageProcessor as ProcessMessage>::Origin, T::Size, T::HeapSize>;

	fn enqueue_message(
		message: BoundedSlice<u8, Self::MaxMessageLen>,
		origin: <T::MessageProcessor as ProcessMessage>::Origin,
	) {
		Self::do_enqueue_message(&origin, message);
		let book_state = BookStateFor::<T>::get(&origin);
		T::QueueChangeHandler::on_queue_changed(origin, book_state.into());
	}

	fn enqueue_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		origin: <T::MessageProcessor as ProcessMessage>::Origin,
	) {
		for message in messages {
			Self::do_enqueue_message(&origin, message);
		}
		let book_state = BookStateFor::<T>::get(&origin);
		T::QueueChangeHandler::on_queue_changed(origin, book_state.into());
	}

	fn sweep_queue(origin: MessageOriginOf<T>) {
		if !BookStateFor::<T>::contains_key(&origin) {
			return
		}
		let mut book_state = BookStateFor::<T>::get(&origin);
		book_state.begin = book_state.end;
		if let Some(neighbours) = book_state.ready_neighbours.take() {
			Self::ready_ring_unknit(&origin, neighbours);
		}
		BookStateFor::<T>::insert(&origin, &book_state);
	}

	fn footprint(origin: MessageOriginOf<T>) -> QueueFootprint {
		BookStateFor::<T>::get(&origin).into()
	}
}
