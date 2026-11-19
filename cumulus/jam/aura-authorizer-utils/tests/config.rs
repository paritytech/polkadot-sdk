// This file is part of Cumulus.

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

//! Slot selection and invalid AURA configuration.

use codec::{DecodeAll, Encode};
use cumulus_aura_authorizer_utils::{AuthConfig, AURA_SLOT_OFFSET};
use jam_types::Slot;

fn encoded_config(size: u32, duration: u32) -> Vec<u8> {
	(Vec::<u32>::new(), 1337u32, [0u8; 32], size, duration).encode()
}

fn config(size: u32, duration: u32) -> AuthConfig {
	AuthConfig::decode_all(&mut &encoded_config(size, duration)[..]).expect("valid config")
}

/// The index is the Aura slot of the anchor, not the raw anchor slot: the round-robin starts
/// counting at the JAM common era, which is `AURA_SLOT_OFFSET` Aura slots after the Unix epoch.
#[test]
fn round_robin_uses_the_aura_slot_of_the_anchor_works() {
	let config = config(3, 2);
	for (slot, expected) in [(0, 0), (1, 0), (2, 1), (4, 2), (5, 2), (6, 0)] {
		assert_eq!(config.expected_collator_index(slot), expected);
	}
	assert_eq!(
		config.expected_collator_index(Slot::MAX),
		(((AURA_SLOT_OFFSET + u64::from(Slot::MAX)) / 2) % 3) as u32
	);
	assert_eq!(config.encode(), encoded_config(3, 2));
}

#[test]
fn round_robin_follows_the_offset_formula_works() {
	// Seven JAM timeslots per para slot does not divide the offset, so the offset shifts every
	// collator's turn against the unshifted `(slot / d) % n`.
	let config = config(2, 7);
	for (slot, expected) in [(0, 1), (1, 1), (6, 0), (7, 0), (11, 1), (14, 1)] {
		assert_eq!(config.expected_collator_index(slot), expected, "slot {slot}");
	}
	assert_ne!(config.expected_collator_index(0), (0 / 7) % 2, "the offset moves the turn");

	for slot in [0, 1, 2, 5, 6, 7, 10, 13, 14, 100, 1_000, Slot::MAX] {
		let aura_slot = (AURA_SLOT_OFFSET + u64::from(slot)) / 7;
		assert_eq!(config.expected_collator_index(slot), (aura_slot % 2) as u32, "slot {slot}");
	}
}

#[test]
fn zero_parameters_errors() {
	for (size, duration) in [(0, 1), (1, 0), (0, 0)] {
		assert!(AuthConfig::decode_all(&mut &encoded_config(size, duration)[..]).is_err());
	}
}
