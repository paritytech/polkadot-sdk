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

#![cfg_attr(not(feature = "std"), no_std)]

//! Substrate time and duration types.
//!
//! Main design goal of this API is to KISS (Keep It Simple and Stupid) and to still fulfill our
//! needs. This means to rely on already existing conventions as much as possible.

use codec::{EncodeLike, FullCodec};
use core::fmt;
use sp_arithmetic::traits::{CheckedAdd, CheckedSub, Saturating, Zero};
use sp_runtime::traits::Member;

/// Provides the current time.
pub trait InstantProvider<I: Instant> {
	/// Returns the current time.
	fn now() -> I;
}

/// Marker trait for `InstantProvider`s where `now() <= now()` always holds.
pub trait MonotonicIncrease {}

/// Marker trait for `InstantProvider`s where `now() < now()` always holds.
pub trait StrictMonotonicIncrease: MonotonicIncrease {}

/// An instant or "time point".
pub trait Instant:
	Member
	+ FullCodec
	+ EncodeLike
	+ fmt::Debug
	+ scale_info::TypeInfo
	+ core::cmp::PartialOrd
	+ core::cmp::Eq
	+ Since<<Self as Instant>::Delta>
	+ Until<<Self as Instant>::Delta>
	// If it turns out that our API is bad, then people can still use UNIX formats:
	+ TryFrom<UnixInstant>
	+ TryInto<UnixInstant>
{
	type Delta: Duration;

	/// Dial time forward by `delta`.
	fn checked_forward(&self, delta: &Self::Delta) -> Option<Self>;

	/// Dial time backward by `delta`.
	fn checked_rewind(&self, delta: &Self::Delta) -> Option<Self>;
}

/// A duration or "time interval".
///
/// Durations MUST always be positive.
pub trait Duration:
	Member
	+ FullCodec
	+ EncodeLike
	+ fmt::Debug
	+ scale_info::TypeInfo
	+ core::cmp::PartialOrd
	+ core::cmp::Eq
	+ CheckedAdd
	+ CheckedSub
	+ Saturating
	+ Zero
	+ TryFrom<UnixDuration>
	+ TryInto<UnixDuration>
{
	/// Scale the duration by a factor.
	fn checked_scale(&self, other: u32) -> Option<Self>;
}

/// Calculates the time since a given instant.
pub trait Since<D: Duration> {
	/// How long it has been since `past`.
	///
	/// `None` is returned if the time is in the future.
	fn since(&self, past: &Self) -> Option<D>;
}

/// Calculates the time until a given instant.
pub trait Until<D: Duration> {
	/// How long it is until `future`.
	///
	/// `None` is returned if the time is in the past.
	fn until(&self, future: &Self) -> Option<D>;
}

/// A UNIX duration.
pub struct UnixDuration {
	/// Nano seconds.
	pub ns: u128,
}

/// A UNIX compatible instant.
///
/// Note that UNIX often uses seconds or milliseconds instead of nanoseconds.
pub struct UnixInstant {
	/// Time since 00:00:00 UTC on 1 January 1970.
	pub since_epoch: UnixDuration,
}
