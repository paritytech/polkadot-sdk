// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Cumulus timestamp related primitives.
//!
//! Provides a [`InherentDataProvider`] that should be used in the validation phase of the
//! parachain. It will be used to create the inherent data and that will be used to check the
//! inherents inside the parachain block (in this case the timestamp inherent). As we don't have
//! access to any clock from the runtime the timestamp is always passed as an inherent into the
//! runtime. To check this inherent when validating the block, we will use the relay chain slot. As
//! the relay chain slot is derived from a timestamp, we can easily convert it back to a timestamp
//! by multiplying it with the slot duration. By comparing the relay chain slot derived timestamp
//! with the timestamp we can ensure that the parachain timestamp is reasonable.

#![cfg_attr(not(feature = "std"), no_std)]

use core::time::Duration;
use cumulus_primitives_core::relay_chain::Slot;
use sp_inherents::{Error, InherentData};

pub use sp_timestamp::{InherentType, INHERENT_IDENTIFIER};

/// The inherent data provider for the timestamp.
///
/// This should be used in the runtime when checking the inherents in the validation phase of the
/// parachain.
pub struct InherentDataProvider {
	relay_chain_slot: Slot,
	relay_chain_slot_duration: Duration,
}

impl InherentDataProvider {
	/// Create `Self` from the given relay chain slot and slot duration.
	pub fn from_relay_chain_slot_and_duration(
		relay_chain_slot: Slot,
		relay_chain_slot_duration: Duration,
	) -> Self {
		Self { relay_chain_slot, relay_chain_slot_duration }
	}

	/// Create the inherent data.
	pub fn create_inherent_data(&self) -> Result<InherentData, Error> {
		let mut inherent_data = InherentData::new();
		self.provide_inherent_data(&mut inherent_data).map(|_| inherent_data)
	}

	/// Provide the inherent data into the given `inherent_data`.
	pub fn provide_inherent_data(&self, inherent_data: &mut InherentData) -> Result<(), Error> {
		// As the parachain starts building at around `relay_chain_slot + 1` we use that slot to
		// calculate the timestamp.
		//
		// Use saturating arithmetic to avoid overflow if inputs are out of range.
		let slot_duration_millis =
			u64::try_from(self.relay_chain_slot_duration.as_millis()).unwrap_or(u64::MAX);
		let next_slot = self.relay_chain_slot.saturating_add(1u64);
		let data: InherentType = (*next_slot).saturating_mul(slot_duration_millis).into();

		inherent_data.put_data(INHERENT_IDENTIFIER, &data)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn timestamp_for(relay_chain_slot: u64, slot_duration: Duration) -> InherentType {
		let inherent_data = InherentDataProvider::from_relay_chain_slot_and_duration(
			Slot::from(relay_chain_slot),
			slot_duration,
		)
		.create_inherent_data()
		.expect("inherent data is created");

		inherent_data
			.get_data(&INHERENT_IDENTIFIER)
			.expect("inherent data decodes")
			.expect("inherent data is present")
	}

	#[test]
	fn zero_slot_duration_is_a_zero_timestamp() {
		assert_eq!(timestamp_for(100, Duration::ZERO), 0u64);
	}

	#[test]
	fn timestamp_is_the_start_of_the_next_relay_chain_slot() {
		assert_eq!(timestamp_for(100, Duration::from_millis(6_000)), 606_000u64);
	}

	#[test]
	fn max_relay_chain_slot_does_not_overflow() {
		assert_eq!(timestamp_for(u64::MAX, Duration::from_millis(6_000)), u64::MAX);
	}

	#[test]
	fn huge_slot_duration_does_not_overflow() {
		assert_eq!(timestamp_for(1, Duration::MAX), u64::MAX);
	}

	#[test]
	fn slot_duration_above_u64_millis_does_not_truncate() {
		// Verify that a duration larger than `u64::MAX` milliseconds saturates instead of wrapping.
		let duration = Duration::new(18_446_744_073_709_551, 616_000_000);
		assert_eq!(duration.as_millis(), u128::from(u64::MAX) + 1);
		assert_eq!(timestamp_for(1, duration), u64::MAX);
	}
}
