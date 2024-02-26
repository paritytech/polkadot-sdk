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

//! Substrate time and duration types.
//!
//! Main design goal of this API is to KISS (Keep It Simple and Stupid) and to still fulfill our
//! needs. This means to rely on already existing conventions as much as possible.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod mock;
pub mod provider;
pub mod unix;

pub use unix::{UnixDuration, UnixInstant};

use codec::FullCodec;
use core::{
	cmp::{Eq, PartialOrd},
	fmt::Debug,
};
use scale_info::TypeInfo;
use sp_arithmetic::traits::{Bounded, CheckedAdd, CheckedSub, Zero};
use sp_runtime::traits::Member;

/// Provides the current time.
// NOTE: we cannot use an associated tye here because it would result in the `Instant cannot be made
// into an object` error.
pub trait InstantProvider<I: Instant> {
	/// Returns the current time.
	fn now() -> I;
}

/// Provide the time at genesis of this chain.
///
/// Can be used to calculate relative times since the inception of the chain. This can be useful for
/// things like vesting or other timed locks.
///
/// It is decoupled from the the normal `InstantProvider` because there can be pallets that only
/// need to know the absolute time.
pub trait GenesisInstantProvider<I: Instant>: InstantProvider<I> {
	/// Returns the time at genesis.
	///
	/// The exact value of this is defined by the runtime.
	fn genesis() -> I;
}

/// Marker trait for `InstantProvider`s where `now() <= now()` always holds.
///
/// `InstantProvider`s must saturate in the overflow case.
pub trait MonotonicIncrease {}

/// Marker trait for `InstantProvider`s where `now() < now()` always holds.
///
/// Note this may not hold in the saturating case.
pub trait StrictMonotonicIncrease: MonotonicIncrease {}

/// An instant or "time point".
pub trait Instant:
	Member
	+ FullCodec
	+ TypeInfo
	+ PartialOrd
	+ Eq
	+ Bounded
	+ Debug
	// If it turns out that our API is bad, then devs can still use UNIX formats:
	+ TryFrom<UnixInstant>
	+ TryInto<UnixInstant>
{
	/// A difference (aka. Delta) between two `Instant`s.
	type Duration: Duration;

	/// Try to increase `self` by `delta`.
	///
	/// This does not use the standard `CheckedAdd` trait since that would require the argument to
	/// be of type `Self`.
	fn checked_add(&self, delta: &Self::Duration) -> Option<Self>;

	fn saturating_add(&self, delta: &Self::Duration) -> Self {
		self.checked_add(delta).unwrap_or_else(|| Self::max_value())
	}

	/// Try to decrease `self` by `delta`.
	fn checked_sub(&self, delta: &Self::Duration) -> Option<Self>;

	fn saturating_sub(&self, delta: &Self::Duration) -> Self {
		self.checked_sub(delta).unwrap_or_else(|| Self::min_value())
	}

	/// How long it has been since `past`.
	///
	/// `None` is returned if the time is in the future. Note that this function glues together the `Self::Duration` and `Self` types.
	fn since(&self, past: &Self) -> Option<Self::Duration>;

	/// How long it is until `future`.
	///
	/// `None` is returned if the time is in the past. Note that this function glues together the `Self::Duration` and `Self` types.
	fn until(&self, future: &Self) -> Option<Self::Duration>;
}

/// A duration or "time interval".
///
/// Durations MUST always be positive.
pub trait Duration:
	Member
	+ FullCodec
	+ TypeInfo
	+ PartialOrd
	+ Eq
	+ Debug
	+ Bounded
	+ CheckedAdd
	+ CheckedSub
	+ Zero
	// If it turns out that our API is bad, then devs can still use UNIX formats:
	+ TryFrom<UnixDuration>
	+ TryInto<UnixDuration>
{
	/// Scale the duration by a factor.
	fn checked_mul_int(&self, other: u128) -> Option<Self>;

	fn saturating_mul_int(&self, other: u128) -> Self {
		self.checked_mul_int(other).unwrap_or_else(|| Self::max_value())
	}

	/// Divide the duration by a factor.
	fn checked_div_int(&self, other: u128) -> Option<Self>;
}
