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

//! Traits for managing message queuing and handling.

use super::storage::Footprint;
use crate::defensive;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, FullCodec, MaxEncodedLen};
use core::{cmp::Ordering, fmt::Debug, marker::PhantomData};
use scale_info::TypeInfo;
use sp_core::{ConstU32, Get, TypedGet};
use sp_runtime::{traits::Convert, BoundedSlice, RuntimeDebug};
use sp_weights::{Weight, WeightMeter};

/// Errors that can happen when attempting to process a message with
/// [`ProcessMessage::process_message()`].
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug)]
pub enum ProcessMessageError {
	/// The message data format is unknown (e.g. unrecognised header)
	BadFormat,
	/// The message data is bad (e.g. decoding returns an error).
	Corrupt,
	/// The message format is unsupported (e.g. old XCM version).
	Unsupported,
	/// Message processing was not attempted because it was not certain that the weight limit
	/// would be respected. The parameter gives the maximum weight which the message could take
	/// to process.
	Overweight(Weight),
	/// The queue wants to give up its current processing slot.
	///
	/// Hints the message processor to cease servicing this queue and proceed to the next
	/// one. This is seen as a *hint*, not an instruction. Implementations must therefore handle
	/// the case that a queue is re-serviced within the same block after *yielding*. A queue is
	/// not required to *yield* again when it is being re-serviced withing the same block.
	Yield,
	/// The message could not be processed for reaching the stack depth limit.
	StackLimitReached,
}

/// Can process messages from a specific origin.
pub trait ProcessMessage {
	/// The transport from where a message originates.
	type Origin: FullCodec + MaxEncodedLen + Clone + Eq + PartialEq + TypeInfo + Debug;

	/// Process the given message, using no more than the remaining `meter` weight to do so.
	///
	/// Returns whether the message was processed.
	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError>;
}

/// Errors that can happen when attempting to execute an overweight message with
/// [`ServiceQueues::execute_overweight()`].
#[derive(Eq, PartialEq, RuntimeDebug)]
pub enum ExecuteOverweightError {
	/// The referenced message was not found.
	NotFound,
	/// The message was already processed.
	///
	/// This can be treated as success condition.
	AlreadyProcessed,
	/// The available weight was insufficient to execute the message.
	InsufficientWeight,
	/// The queue is paused and no message can be executed from it.
	///
	/// This can change at any time and may resolve in the future by re-trying.
	QueuePaused,
	/// An unspecified error.
	Other,
	/// Another call is currently ongoing and prevents this call from executing.
	RecursiveDisallowed,
}

/// Can service queues and execute overweight messages.
pub trait ServiceQueues {
	/// Addresses a specific overweight message.
	type OverweightMessageAddress;

	/// Service all message queues in some fair manner.
	///
	/// - `weight_limit`: The maximum amount of dynamic weight that this call can use.
	///
	/// Returns the dynamic weight used by this call; is never greater than `weight_limit`.
	/// Should only be called in top-level runtime entry points like `on_initialize` or `on_idle`.
	/// Otherwise, stack depth limit errors may be miss-handled.
	fn service_queues(weight_limit: Weight) -> Weight;

	/// Executes a message that could not be executed by [`Self::service_queues()`] because it was
	/// temporarily overweight.
	fn execute_overweight(
		_weight_limit: Weight,
		_address: Self::OverweightMessageAddress,
	) -> Result<Weight, ExecuteOverweightError> {
		Err(ExecuteOverweightError::NotFound)
	}
}

/// Services queues by doing nothing.
pub struct NoopServiceQueues<OverweightAddr>(PhantomData<OverweightAddr>);
impl<OverweightAddr> ServiceQueues for NoopServiceQueues<OverweightAddr> {
	type OverweightMessageAddress = OverweightAddr;

	fn service_queues(_: Weight) -> Weight {
		Weight::zero()
	}
}

/// Can enqueue messages for multiple origins.
pub trait EnqueueMessage<Origin: MaxEncodedLen> {
	/// The maximal length any enqueued message may have.
	type MaxMessageLen: Get<u32>;

	/// Enqueue a single `message` from a specific `origin`.
	fn enqueue_message(message: BoundedSlice<u8, Self::MaxMessageLen>, origin: Origin);

	/// Enqueue multiple `messages` from a specific `origin`.
	fn enqueue_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		origin: Origin,
	);

	/// Any remaining unprocessed messages should happen only lazily, not proactively.
	fn sweep_queue(origin: Origin);
}

impl<Origin: MaxEncodedLen> EnqueueMessage<Origin> for () {
	type MaxMessageLen = ConstU32<0>;
	fn enqueue_message(_: BoundedSlice<u8, Self::MaxMessageLen>, _: Origin) {}
	fn enqueue_messages<'a>(
		_: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		_: Origin,
	) {
	}
	fn sweep_queue(_: Origin) {}
}

/// The resource footprint of a queue.
#[derive(Default, Copy, Clone, Eq, PartialEq, RuntimeDebug)]
pub struct QueueFootprint {
	/// The number of pages in the queue (including overweight pages).
	pub pages: u32,
	/// The number of pages that are ready (not yet processed and also not overweight).
	pub ready_pages: u32,
	/// The storage footprint of the queue (including overweight messages).
	pub storage: Footprint,
}

/// The resource footprint of a batch of messages.
#[derive(Default, Copy, Clone, PartialEq, RuntimeDebug)]
pub struct BatchFootprint {
	/// The number of messages in the batch.
	pub msgs_count: usize,
	/// The total size in bytes of all the messages in the batch.
	pub size_in_bytes: usize,
	/// The number of resulting new pages in the queue if the current batch was added.
	pub new_pages_count: u32,
}

/// The resource footprints of continuous subsets of messages.
///
/// For a set of messages `xcms[0..n]`, each `footprints[i]` contains the footprint
/// of the batch `xcms[0..i]`, so as `i` increases `footprints[i]` contains the footprint
/// of a bigger batch.
#[derive(Default, RuntimeDebug)]
pub struct BatchesFootprints {
	/// The position in the first available MQ page where the batch will start being appended.
	///
	/// The messages in the batch will be enqueued to the message queue. Since the message queue is
	/// organized in pages, the messages may be enqueued across multiple contiguous pages.
	/// The position where we start appending messages to the first available MQ page is of
	/// particular importance since it impacts the performance of the enqueuing operation.
	/// That's because the first page has to be decoded first. This is not needed for the following
	/// pages.
	pub first_page_pos: usize,
	pub footprints: Vec<BatchFootprint>,
}

impl BatchesFootprints {
	/// Appends a batch footprint to the back of the collection.
	///
	/// The new footprint represents a batch that includes all the messages contained by the
	/// previous batches plus the provided `msg`. If `new_page` is true, we will consider that
	/// the provided `msg` is appended to a new message queue page. Otherwise, we consider
	/// that it is appended to the current page.
	pub fn push(&mut self, msg: &[u8], new_page: bool) {
		let previous_footprint =
			self.footprints.last().map(|footprint| *footprint).unwrap_or_default();

		let mut new_pages_count = previous_footprint.new_pages_count;
		if new_page {
			new_pages_count = new_pages_count.saturating_add(1);
		}
		self.footprints.push(BatchFootprint {
			msgs_count: previous_footprint.msgs_count.saturating_add(1),
			size_in_bytes: previous_footprint.size_in_bytes.saturating_add(msg.len()),
			new_pages_count,
		});
	}

	/// Gets the biggest batch for which the comparator function returns `Ordering::Less`.
	pub fn search_best_by<F>(&self, f: F) -> &BatchFootprint
	where
		F: FnMut(&BatchFootprint) -> Ordering,
	{
		// Since the batches are sorted by size, we can use binary search.
		let maybe_best_idx = match self.footprints.binary_search_by(f) {
			Ok(last_ok_idx) => Some(last_ok_idx),
			Err(first_err_idx) => first_err_idx.checked_sub(1),
		};
		if let Some(best_idx) = maybe_best_idx {
			match self.footprints.get(best_idx) {
				Some(best_footprint) => return best_footprint,
				None => {
					defensive!("Invalid best_batch_idx: {}", best_idx);
				},
			}
		}
		&BatchFootprint { msgs_count: 0, size_in_bytes: 0, new_pages_count: 0 }
	}
}

/// Provides information on queue footprint.
pub trait QueueFootprintQuery<Origin> {
	/// The maximal length any enqueued message may have.
	type MaxMessageLen: Get<u32>;

	/// Return the state footprint of the given queue.
	fn footprint(origin: Origin) -> QueueFootprint;

	/// Get the `BatchFootprint` for each batch of messages `[0..n]`
	/// as long as the total number of pages would be <= `total_pages_limit`.
	///
	/// # Examples
	///
	/// Let's consider that each message would result in a new page and that there's already 1
	/// full page in the queue. Then, for the messages `["1", "2", "3"]`
	/// and `total_pages_limit = 3`, `get_batches_footprints()` would return:
	/// ```
	/// use frame_support::traits::BatchFootprint;
	///
	/// vec![
	/// 	// The footprint of batch ["1"]
	/// 	BatchFootprint {
	/// 		msgs_count: 1,
	/// 		size_in_bytes: 1,
	/// 		new_pages_count: 1, // total pages count = 2
	/// 	},
	/// 	// The footprint of batch ["1", "2"]
	/// 	BatchFootprint {
	/// 		msgs_count: 2,
	/// 		size_in_bytes: 2,
	/// 		new_pages_count: 2, // total pages count = 3
	/// 	}
	/// 	// For the batch ["1", "2", "3"], the total pages count would be 4, which would exceed
	/// 	// the `total_pages_limit`.
	/// ];
	/// ```
	fn get_batches_footprints<'a>(
		origin: Origin,
		msgs: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		total_pages_limit: u32,
	) -> BatchesFootprints;
}

impl<Origin: MaxEncodedLen> QueueFootprintQuery<Origin> for () {
	type MaxMessageLen = ConstU32<0>;

	fn footprint(_: Origin) -> QueueFootprint {
		QueueFootprint::default()
	}

	fn get_batches_footprints<'a>(
		_origin: Origin,
		_msgs: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		_total_pages_limit: u32,
	) -> BatchesFootprints {
		BatchesFootprints::default()
	}
}

/// Transform the origin of an [`EnqueueMessage`] via `C::convert`.
pub struct TransformOrigin<E, O, N, C>(PhantomData<(E, O, N, C)>);
impl<E: EnqueueMessage<O>, O: MaxEncodedLen, N: MaxEncodedLen, C: Convert<N, O>> EnqueueMessage<N>
	for TransformOrigin<E, O, N, C>
{
	type MaxMessageLen = E::MaxMessageLen;

	fn enqueue_message(message: BoundedSlice<u8, Self::MaxMessageLen>, origin: N) {
		E::enqueue_message(message, C::convert(origin));
	}

	fn enqueue_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		origin: N,
	) {
		E::enqueue_messages(messages, C::convert(origin));
	}

	fn sweep_queue(origin: N) {
		E::sweep_queue(C::convert(origin));
	}
}

impl<E: QueueFootprintQuery<O>, O: MaxEncodedLen, N: MaxEncodedLen, C: Convert<N, O>>
	QueueFootprintQuery<N> for TransformOrigin<E, O, N, C>
{
	type MaxMessageLen = E::MaxMessageLen;

	fn footprint(origin: N) -> QueueFootprint {
		E::footprint(C::convert(origin))
	}

	fn get_batches_footprints<'a>(
		origin: N,
		msgs: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		total_pages_limit: u32,
	) -> BatchesFootprints {
		E::get_batches_footprints(C::convert(origin), msgs, total_pages_limit)
	}
}

/// Handles incoming messages for a single origin.
pub trait HandleMessage {
	/// The maximal length any enqueued message may have.
	type MaxMessageLen: Get<u32>;

	/// Enqueue a single `message` with an implied origin.
	fn handle_message(message: BoundedSlice<u8, Self::MaxMessageLen>);

	/// Enqueue multiple `messages` from an implied origin.
	fn handle_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
	);

	/// Any remaining unprocessed messages should happen only lazily, not proactively.
	fn sweep_queue();
}

/// Adapter type to transform an [`EnqueueMessage`] with an origin into a [`HandleMessage`] impl.
pub struct EnqueueWithOrigin<E, O>(PhantomData<(E, O)>);
impl<E: EnqueueMessage<O::Type>, O: TypedGet> HandleMessage for EnqueueWithOrigin<E, O>
where
	O::Type: MaxEncodedLen,
{
	type MaxMessageLen = E::MaxMessageLen;

	fn handle_message(message: BoundedSlice<u8, Self::MaxMessageLen>) {
		E::enqueue_message(message, O::get());
	}

	fn handle_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
	) {
		E::enqueue_messages(messages, O::get());
	}

	fn sweep_queue() {
		E::sweep_queue(O::get());
	}
}

/// Provides information on paused queues.
pub trait QueuePausedQuery<Origin> {
	/// Whether this queue is paused.
	fn is_paused(origin: &Origin) -> bool;
}

#[impl_trait_for_tuples::impl_for_tuples(8)]
impl<Origin> QueuePausedQuery<Origin> for Tuple {
	fn is_paused(origin: &Origin) -> bool {
		for_tuples!( #(
			if Tuple::is_paused(origin) {
				return true;
			}
		)* );
		false
	}
}
