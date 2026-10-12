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

//! Default weights for `pallet_validator_set_announcer`.
//!
//! Runtimes should use their own benchmarked weights.

#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions needed for `pallet_validator_set_announcer`.
pub trait WeightInfo {
	fn announce(n: u32) -> Weight;
	fn send_announcement(n: u32) -> Weight;
}

/// Default weights for `pallet_validator_set_announcer`, see the module doc for their source.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:1)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn announce(n: u32) -> Weight {
		Weight::from_parts(27_026_338, 33_491)
			.saturating_add(Weight::from_parts(130_496, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	/// Storage: `XcmpQueue::DeliveryFeeFactor` (r:1 w:0)
	/// Proof: `XcmpQueue::DeliveryFeeFactor` (`max_values`: None, `max_size`: Some(28), added:
	/// 2503, mode: `MaxEncodedLen`)
	/// Storage: `PolkadotXcm::SupportedVersion` (r:1 w:0)
	/// Proof: `PolkadotXcm::SupportedVersion` (`max_values`: None, `max_size`: None, mode:
	/// `Measured`)
	/// Storage: `XcmpQueue::OutboundXcmpStatus` (r:1 w:1)
	/// Proof: `XcmpQueue::OutboundXcmpStatus` (`max_values`: Some(1), `max_size`: Some(2306),
	/// added: 2801, mode: `MaxEncodedLen`)
	/// Storage: `ParachainSystem::RelevantMessagingState` (r:1 w:0)
	/// Proof: `ParachainSystem::RelevantMessagingState` (`max_values`: Some(1), `max_size`: None,
	/// mode: `Measured`)
	/// Storage: `XcmpQueue::OutboundXcmpMessages` (r:1 w:1)
	/// Proof: `XcmpQueue::OutboundXcmpMessages` (`max_values`: None, `max_size`: Some(105506),
	/// added: 107981, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn send_announcement(n: u32) -> Weight {
		Weight::from_parts(42_127_876, 108_971)
			.saturating_add(Weight::from_parts(222_975, 64).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(5_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `ValidatorCollators::ValidatorSet` (r:1 w:1)
	/// Proof: `ValidatorCollators::ValidatorSet` (`max_values`: Some(1), `max_size`: Some(32006),
	/// added: 32501, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorCollators::PendingRotation` (r:0 w:1)
	/// Proof: `ValidatorCollators::PendingRotation` (`max_values`: Some(1), `max_size`: Some(1),
	/// added: 496, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn announce(n: u32) -> Weight {
		Weight::from_parts(27_026_338, 33_491)
			.saturating_add(Weight::from_parts(130_496, 0).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
	/// Storage: `XcmpQueue::DeliveryFeeFactor` (r:1 w:0)
	/// Proof: `XcmpQueue::DeliveryFeeFactor` (`max_values`: None, `max_size`: Some(28), added:
	/// 2503, mode: `MaxEncodedLen`)
	/// Storage: `PolkadotXcm::SupportedVersion` (r:1 w:0)
	/// Proof: `PolkadotXcm::SupportedVersion` (`max_values`: None, `max_size`: None, mode:
	/// `Measured`)
	/// Storage: `XcmpQueue::OutboundXcmpStatus` (r:1 w:1)
	/// Proof: `XcmpQueue::OutboundXcmpStatus` (`max_values`: Some(1), `max_size`: Some(2306),
	/// added: 2801, mode: `MaxEncodedLen`)
	/// Storage: `ParachainSystem::RelevantMessagingState` (r:1 w:0)
	/// Proof: `ParachainSystem::RelevantMessagingState` (`max_values`: Some(1), `max_size`: None,
	/// mode: `Measured`)
	/// Storage: `XcmpQueue::OutboundXcmpMessages` (r:1 w:1)
	/// Proof: `XcmpQueue::OutboundXcmpMessages` (`max_values`: None, `max_size`: Some(105506),
	/// added: 107981, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[1, 1000]`.
	fn send_announcement(n: u32) -> Weight {
		Weight::from_parts(42_127_876, 108_971)
			.saturating_add(Weight::from_parts(222_975, 64).saturating_mul(n.into()))
			.saturating_add(RocksDbWeight::get().reads(5_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
}
