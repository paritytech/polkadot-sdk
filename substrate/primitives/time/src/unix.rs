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

use super::*;
use codec::{Decode, Encode};
use core::ops::{Add, Sub};
use scale_info::TypeInfo;

/// A UNIX duration.
#[derive(Encode, Decode, Default, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, TypeInfo)]
#[cfg_attr(feature = "std", derive(arbitrary::Arbitrary))]
pub struct UnixDuration {
	/// Nano seconds.
	pub ns: u128,
}

impl UnixDuration {
	pub fn from_millis<T: Into<u128>>(ms: T) -> Self {
		Self { ns: ms.into().saturating_mul(1_000_000) }
	}
}

/// A UNIX compatible instant.
///
/// Note that UNIX often uses seconds or milliseconds instead of nanoseconds.
#[derive(Encode, Decode, Default, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, TypeInfo)]
#[cfg_attr(feature = "std", derive(arbitrary::Arbitrary))]
pub struct UnixInstant {
	/// Time since 00:00:00 UTC on 1 January 1970.
	pub since_epoch: UnixDuration,
}

impl UnixInstant {
	pub fn from_epoch_start<D: Into<UnixDuration>>(d: D) -> Self {
		Self { since_epoch: d.into() }
	}
}

impl Instant for UnixInstant {
	type Duration = UnixDuration;

	fn checked_add(&self, other: &Self::Duration) -> Option<Self> {
		self.since_epoch
			.ns
			.checked_add(other.ns)
			.map(|ns| UnixInstant { since_epoch: UnixDuration { ns } })
	}

	fn checked_sub(&self, other: &Self::Duration) -> Option<Self> {
		self.since_epoch
			.ns
			.checked_sub(other.ns)
			.map(|ns| UnixInstant { since_epoch: UnixDuration { ns } })
	}

	fn since(&self, past: &Self) -> Option<Self::Duration> {
		self.since_epoch
			.ns
			.checked_sub(past.since_epoch.ns)
			.map(|ns| UnixDuration { ns })
	}

	fn until(&self, future: &Self) -> Option<Self::Duration> {
		future
			.since_epoch
			.ns
			.checked_sub(self.since_epoch.ns)
			.map(|ns| UnixDuration { ns })
	}
}

impl Bounded for UnixInstant {
	fn min_value() -> Self {
		Self { since_epoch: Bounded::min_value() }
	}

	fn max_value() -> Self {
		Self { since_epoch: Bounded::max_value() }
	}
}

impl Bounded for UnixDuration {
	fn min_value() -> Self {
		Self { ns: 0 }
	}

	fn max_value() -> Self {
		Self { ns: u128::max_value() }
	}
}

impl Zero for UnixDuration {
	fn is_zero(&self) -> bool {
		self == &Self::default()
	}

	fn zero() -> Self {
		Self::default()
	}
}

impl Add for UnixDuration {
	type Output = Self;

	fn add(self, rhs: Self) -> Self::Output {
		Self { ns: self.ns + rhs.ns }
	}
}

impl Sub for UnixDuration {
	type Output = Self;

	fn sub(self, rhs: Self) -> Self::Output {
		Self { ns: self.ns - rhs.ns }
	}
}

impl CheckedAdd for UnixDuration {
	fn checked_add(&self, rhs: &Self) -> Option<Self> {
		self.ns.checked_add(rhs.ns).map(|ns| UnixDuration { ns })
	}
}

impl CheckedSub for UnixDuration {
	fn checked_sub(&self, rhs: &Self) -> Option<Self> {
		self.ns.checked_sub(rhs.ns).map(|ns| UnixDuration { ns })
	}
}

impl Duration for UnixDuration {
	fn checked_mul_int(&self, scale: u128) -> Option<Self> {
		self.ns.checked_mul(scale.into()).map(|ns| UnixDuration { ns })
	}

	fn checked_div_int(&self, scale: u128) -> Option<Self> {
		self.ns.checked_div(scale.into()).map(|ns| UnixDuration { ns })
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fuzz() {
		crate::mock::InstantFuzzer::<UnixInstant>::fuzz();
	}
}
