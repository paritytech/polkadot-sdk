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

use crate::{Duration, Instant, InstantProvider};
use sp_runtime::traits::Get;

/// An `InstantProvider` that follows a linear equation on the form of `now() = s * N::get() + o`.
///
/// This can be used by any chain that has constant block times. Mostly relay- and solo-chains.
pub struct LinearInstantProvider<I, T, N, O, S>(core::marker::PhantomData<(I, T, N, O, S)>);

impl<I, T, N, O, S> InstantProvider<I> for LinearInstantProvider<I, T, N, O, S>
where
	I: Instant,
	T: Into<u128>,
	N: Get<T>,
	O: Get<I>,
	S: Get<I::Duration>,
{
	fn now() -> I {
		let slope = S::get().saturating_mul_int(N::get().into());
		let offset = O::get();

		offset.saturating_add(&slope)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{UnixDuration, UnixInstant};

	#[test]
	fn linear_instant_provider_works() {
		struct BlockNumber;
		impl Get<u32> for BlockNumber {
			fn get() -> u32 {
				42
			}
		}

		struct Offset;
		impl Get<UnixInstant> for Offset {
			fn get() -> UnixInstant {
				UnixInstant { since_epoch: UnixDuration { ns: 654 } }
			}
		}

		struct Slope;
		impl Get<UnixDuration> for Slope {
			fn get() -> UnixDuration {
				UnixDuration { ns: 123 }
			}
		}

		type Time = LinearInstantProvider<UnixInstant, u32, BlockNumber, Offset, Slope>;
		assert_eq!(Time::now(), UnixInstant { since_epoch: UnixDuration { ns: 654 + 42 * 123 } });
	}

	#[test]
	fn linear_instant_overflow_saturates_works() {
		struct BlockNumber;
		impl Get<u32> for BlockNumber {
			fn get() -> u32 {
				42
			}
		}

		struct Offset;
		impl Get<UnixInstant> for Offset {
			fn get() -> UnixInstant {
				UnixInstant { since_epoch: UnixDuration { ns: 2 } }
			}
		}

		struct Slope;
		impl Get<UnixDuration> for Slope {
			fn get() -> UnixDuration {
				UnixDuration { ns: u128::MAX / 2 }
			}
		}

		type Time = LinearInstantProvider<UnixInstant, u32, BlockNumber, Offset, Slope>;
		assert_eq!(Time::now(), UnixInstant { since_epoch: UnixDuration { ns: u128::MAX } });
	}
}
