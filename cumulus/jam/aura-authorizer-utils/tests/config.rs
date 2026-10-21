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

//! Slot selection and invalid AURA configuration.

use codec::{DecodeAll, Encode};
use cumulus_aura_authorizer_utils::AuthConfig;
use jam_types::Slot;

fn encoded_config(size: u32, duration: u32) -> Vec<u8> {
	(Vec::<u32>::new(), 1337u32, [0u8; 32], size, duration).encode()
}

#[test]
fn round_robin_works() {
	let config = AuthConfig::decode_all(&mut &encoded_config(3, 2)[..]).expect("valid config");
	for (slot, expected) in [(0, 0), (1, 0), (2, 1), (4, 2), (5, 2), (6, 0)] {
		assert_eq!(config.expected_collator_index(slot), expected);
	}
	assert_eq!(config.expected_collator_index(Slot::MAX), (Slot::MAX / 2) % 3);
	assert_eq!(config.encode(), encoded_config(3, 2));
}

#[test]
fn zero_parameters_errors() {
	for (size, duration) in [(0, 1), (1, 0), (0, 0)] {
		assert!(AuthConfig::decode_all(&mut &encoded_config(size, duration)[..]).is_err());
	}
}
