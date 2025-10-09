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

//! Slot adjustment logic for pallet-aura integration
//!
//! This module provides functionality to properly adjust the CurrentSlot
//! in pallet-aura when SlotDuration changes.

use sp_consensus_aura::Slot;

/// Recalculate the current slot based on new slot duration.
/// 
/// This is necessary when slot duration changes to ensure that the
/// CurrentSlot in pallet-aura remains consistent with the actual time.
///
/// # Parameters
/// 
/// - `current_slot`: The current slot number from pallet-aura
/// - `old_duration`: Previous slot duration in milliseconds  
/// - `new_duration`: New slot duration in milliseconds
/// - `current_timestamp`: Current block timestamp
/// 
/// # Returns
/// 
/// The adjusted slot number that corresponds to the current timestamp
/// with the new slot duration.
///
/// # Example
/// 
/// ```ignore
/// let current_slot = 10u64; // Current slot
/// let old_duration = 6000u64; // 6 seconds  
/// let new_duration = 12000u64; // 12 seconds
/// let timestamp = 60000u64; // 60 seconds
/// 
/// let adjusted = recalculate_current_slot(
///     current_slot, 
///     old_duration, 
///     new_duration, 
///     timestamp
/// );
/// // adjusted should be 5 (60000 / 12000)
/// ```
pub fn recalculate_current_slot(
	current_slot: u64,
	old_duration_ms: u64,
	new_duration_ms: u64,
	current_timestamp_ms: u64,
) -> u64 {
	// If durations are the same, no adjustment needed
	if old_duration_ms == new_duration_ms {
		return current_slot;
	}

	// If new duration is zero, something is wrong
	if new_duration_ms == 0 {
		return current_slot;
	}

	// Calculate the new slot based on current timestamp and new duration
	let new_slot = current_timestamp_ms / new_duration_ms;

	new_slot
}

/// Calculate slot from timestamp and duration (matches sp-consensus-slots logic)
pub fn slot_from_timestamp(timestamp_ms: u64, slot_duration_ms: u64) -> Slot {
	if slot_duration_ms == 0 {
		return Slot::from(0u64);
	}
	Slot::from(timestamp_ms / slot_duration_ms)
}

/// Calculate timestamp from slot and duration
pub fn timestamp_from_slot(slot: Slot, slot_duration_ms: u64) -> Option<u64> {
	slot_duration_ms.checked_mul(*slot)
}

/// Validate that the slot adjustment is reasonable
pub fn validate_slot_adjustment(
	old_slot: u64,
	new_slot: u64,
	old_duration_ms: u64,
	new_duration_ms: u64,
) -> bool {
	// Check that the adjustment makes sense given the duration change
	
	// If duration increased, slot should decrease (or stay similar)
	if new_duration_ms > old_duration_ms {
		return new_slot <= old_slot;
	}
	
	// If duration decreased, slot should increase (or stay similar)  
	if new_duration_ms < old_duration_ms {
		return new_slot >= old_slot;
	}
	
	// If durations equal, slots should be equal
	new_slot == old_slot
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_recalculate_current_slot_duration_doubled() {
		// Duration doubled: 6s -> 12s
		// Time 60s: slot should go from 10 to 5
		let result = recalculate_current_slot(10, 6000, 12000, 60000);
		assert_eq!(result, 5);
	}

	#[test]
	fn test_recalculate_current_slot_duration_halved() {
		// Duration halved: 12s -> 6s  
		// Time 60s: slot should go from 5 to 10
		let result = recalculate_current_slot(5, 12000, 6000, 60000);
		assert_eq!(result, 10);
	}

	#[test]
	fn test_recalculate_current_slot_same_duration() {
		// Duration unchanged: should return same slot
		let result = recalculate_current_slot(10, 6000, 6000, 60000);
		assert_eq!(result, 10);
	}

	#[test]
	fn test_slot_from_timestamp() {
		let slot = slot_from_timestamp(60000, 6000);
		assert_eq!(*slot, 10);

		let slot = slot_from_timestamp(60000, 12000);
		assert_eq!(*slot, 5);
	}

	#[test]
	fn test_timestamp_from_slot() {
		let timestamp = timestamp_from_slot(Slot::from(10u64), 6000);
		assert_eq!(timestamp, Some(60000));

		let timestamp = timestamp_from_slot(Slot::from(5u64), 12000);
		assert_eq!(timestamp, Some(60000));
	}

	#[test]
	fn test_validate_slot_adjustment() {
		// Duration doubled: slot should decrease
		assert!(validate_slot_adjustment(10, 5, 6000, 12000));
		assert!(!validate_slot_adjustment(10, 15, 6000, 12000));

		// Duration halved: slot should increase
		assert!(validate_slot_adjustment(5, 10, 12000, 6000));
		assert!(!validate_slot_adjustment(5, 3, 12000, 6000));

		// Duration same: slot should stay same
		assert!(validate_slot_adjustment(10, 10, 6000, 6000));
		assert!(!validate_slot_adjustment(10, 5, 6000, 6000));
	}
}
