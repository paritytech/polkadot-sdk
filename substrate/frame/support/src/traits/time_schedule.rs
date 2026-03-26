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

//! Traits and associated utilities for time-based scheduling of dispatchables in FRAME.

#[allow(deprecated)]
use super::PreimageProvider;
use codec::{Codec, Decode, DecodeWithMemTracking, Encode, EncodeLike, MaxEncodedLen};
use core::fmt::Debug;
use scale_info::TypeInfo;
use sp_runtime::{traits::Saturating, DispatchError, RuntimeDebug};

/// Information relating to the period of a scheduled task. First item is the duration
/// between executions and the second is the number of times it should be executed in
/// total before the task is considered finished and removed.
pub type Period<Time> = (Time, u32);

/// Priority with which a call is scheduled. It's just a linear amount with lowest values meaning
/// higher priority.
pub type Priority = u8;

/// The dispatch time of a scheduled task.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Copy,
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum DispatchTime<Time> {
	/// At specified timestamp.
	At(Time),
	/// After specified duration.
	After(Time),
}

impl<Time: Saturating + Copy> DispatchTime<Time> {
	pub fn evaluate(&self, now: Time) -> Time {
		match &self {
			Self::At(m) => *m,
			Self::After(m) => m.saturating_add(now),
		}
	}
}

/// The highest priority. We invert the value so that normal sorting will place the highest
/// priority at the beginning of the list.
pub const HIGHEST_PRIORITY: Priority = 0;
/// Anything of this value or lower will definitely be scheduled on the block that they ask for,
/// even if it breaches the `MaximumWeight` limitation.
pub const HARD_DEADLINE: Priority = 63;
/// The lowest priority. Most stuff should be around here.
pub const LOWEST_PRIORITY: Priority = 255;

/// Type representing an encodable value or the hash of the encoding of such a value.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum MaybeHashed<T, Hash> {
	/// The value itself.
	Value(T),
	/// The hash of the encoded value which this value represents.
	Hash(Hash),
}

impl<T, H> From<T> for MaybeHashed<T, H> {
	fn from(t: T) -> Self {
		MaybeHashed::Value(t)
	}
}

/// Error type for `MaybeHashed::lookup`.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum LookupError {
	/// A call of this hash was not known.
	Unknown,
	/// The preimage for this hash was known but could not be decoded into a `Call`.
	BadFormat,
}

impl<T: Decode, H> MaybeHashed<T, H> {
	pub fn as_value(&self) -> Option<&T> {
		match &self {
			Self::Value(c) => Some(c),
			Self::Hash(_) => None,
		}
	}

	pub fn as_hash(&self) -> Option<&H> {
		match &self {
			Self::Value(_) => None,
			Self::Hash(h) => Some(h),
		}
	}

	pub fn ensure_requested<P: PreimageProvider<H>>(&self) {
		match &self {
			Self::Value(_) => (),
			Self::Hash(hash) => P::request_preimage(hash),
		}
	}

	pub fn ensure_unrequested<P: PreimageProvider<H>>(&self) {
		match &self {
			Self::Value(_) => (),
			Self::Hash(hash) => P::unrequest_preimage(hash),
		}
	}

	pub fn resolved<P: PreimageProvider<H>>(self) -> (Self, Option<H>) {
		match self {
			Self::Value(c) => (Self::Value(c), None),
			Self::Hash(h) => {
				let data = match P::get_preimage(&h) {
					Some(p) => p,
					None => return (Self::Hash(h), None),
				};
				match T::decode(&mut &data[..]) {
					Ok(c) => (Self::Value(c), Some(h)),
					Err(_) => (Self::Hash(h), None),
				}
			},
		}
	}
}

pub mod v1 {
	use super::*;
	use crate::traits::Bounded;

	/// A type that can be used as a time-based scheduler.
	///
	/// Tasks are scheduled at a given `Time` (milliseconds since Unix epoch) and dispatched when
	/// the chain's timestamp reaches that value. The scheduler groups tasks into buckets of
	/// configurable resolution, so actual dispatch happens at the start of the first block whose
	/// timestamp falls within or after the target bucket.
	///
	/// - `Time`: The timestamp type (ms since Unix epoch).
	/// - `Call`: The dispatchable call type.
	/// - `Origin`: The origin type used to authorize the scheduling.
	pub trait Anon<Time, Call, Origin> {
		/// An address which can be used for removing a scheduled task.
		type Address: Codec + MaxEncodedLen + Clone + Eq + EncodeLike + Debug + TypeInfo;
		/// The hasher used in the runtime.
		type Hasher: sp_runtime::traits::Hash;

		/// Schedule a dispatch to happen at the beginning of some block in the future.
		///
		/// This is not named.
		fn schedule(
			when: DispatchTime<Time>,
			maybe_periodic: Option<Period<Time>>,
			priority: Priority,
			origin: Origin,
			call: Bounded<Call, Self::Hasher>,
		) -> Result<Self::Address, DispatchError>;

		/// Cancel a scheduled task. If periodic, then it will cancel all further instances of that,
		/// also.
		///
		/// Will return an `Unavailable` error if the `address` is invalid.
		///
		/// NOTE: This guaranteed to work only *before* the point that it is due to be executed.
		/// If it ends up being delayed beyond the point of execution, then it cannot be cancelled.
		///
		/// NOTE2: This will not work to cancel periodic tasks after their initial execution. For
		/// that, you must name the task explicitly using the `Named` trait.
		fn cancel(address: Self::Address) -> Result<(), DispatchError>;

		/// Reschedule a task. For one-off tasks, this dispatch is guaranteed to succeed
		/// only if it is executed *before* the currently scheduled block. For periodic tasks,
		/// this dispatch is guaranteed to succeed only before the *initial* execution; for
		/// others, use `reschedule_named`.
		///
		/// Will return an `Unavailable` error if the `address` is invalid.
		fn reschedule(
			address: Self::Address,
			when: DispatchTime<Time>,
		) -> Result<Self::Address, DispatchError>;

		/// Return the next dispatch time for a given task.
		///
		/// Will return an `Unavailable` error if the `address` is invalid.
		fn next_dispatch_time(address: Self::Address) -> Result<Time, DispatchError>;
	}

	pub type TaskName = [u8; 32];

	/// A type that can be used as a scheduler.
	///
	/// - `Time`: Timestamp type.
	pub trait Named<Time, Call, Origin> {
		/// An address which can be used for removing a scheduled task.
		type Address: Codec + MaxEncodedLen + Clone + Eq + EncodeLike + Debug;
		/// The hasher used in the runtime.
		type Hasher: sp_runtime::traits::Hash;

		/// Schedule a named dispatch to happen at the given timestamp.
		///
		/// - `id`: A unique identifier for the task. Returns an error if a task with this id
		///   already exists.
		///
		/// NOTE: This will request `call` to be made available.
		fn schedule_named(
			id: TaskName,
			when: DispatchTime<Time>,
			maybe_periodic: Option<Period<Time>>,
			priority: Priority,
			origin: Origin,
			call: Bounded<Call, Self::Hasher>,
		) -> Result<Self::Address, DispatchError>;

		/// Cancel a scheduled, named task. If periodic, then it will cancel all further instances
		/// of that, also.
		///
		/// Will return an `Unavailable` error if the `id` is invalid.
		///
		/// NOTE: This guaranteed to work only *before* the point that it is due to be executed.
		/// If it ends up being delayed beyond the point of execution, then it cannot be cancelled.
		fn cancel_named(id: TaskName) -> Result<(), DispatchError>;

		/// Reschedule a task. For one-off tasks, this dispatch is guaranteed to succeed
		/// only if it is executed *before* the currently scheduled block.
		///
		/// Will return an `Unavailable` error if the `id` is invalid.
		fn reschedule_named(
			id: TaskName,
			when: DispatchTime<Time>,
		) -> Result<Self::Address, DispatchError>;

		/// Return the next dispatch time for a given task.
		///
		/// Will return an `Unavailable` error if the `id` is invalid.
		fn next_dispatch_time(id: TaskName) -> Result<Time, DispatchError>;
	}
}
