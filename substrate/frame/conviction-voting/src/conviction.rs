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

//! The conviction datatype.

use crate::types::Delegations;
use sp_runtime::traits::Convert;

/// Convert a conviction into a lock duration.
pub trait AsLockDuration {
	type Duration;

	/// Convert the conviction to a lock duration.
	fn as_locked_duration(&self) -> Self::Duration;
}

/// Convert a balance with a conviction into votes.
pub(super) fn as_delegations<
	C: Convert<(Conviction, Balance), Balance>,
	Conviction,
	Balance: Clone,
>(
	conviction: Conviction,
	capital: Balance,
) -> Delegations<Balance> {
	let votes = C::convert((conviction, capital.clone()));
	Delegations { votes, capital }
}

// FAIL-CI remove TryFrom<u8> from debugging, and copy
/*pub trait ConvictionTrait: AsLockDuration + AsConvictedVotes + Copy + Zero + Bounded + Clone + PartialEq + TryFrom<u8> + TypeInfo + MaxEncodedLen + Encode + Decode + frame_support::Parameter + frame_support::pallet_prelude::Member {
	/// Calculate the votes that result from a balance and a conviction.
	fn votes<B>(
		self,
		capital: B,
	) -> Delegations<B>
		where Self: AsConvictedVotes<Balance = B>,
	{
		Delegations { votes: self.as_votes(&capital), capital }
	}
}
*/
