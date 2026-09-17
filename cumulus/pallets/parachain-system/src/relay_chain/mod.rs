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

//! Relay-chain-specific functionality of the parachain-system pallet.
//!
//! Owns the V2 relay-state proof machinery, BABE descendant validation, V3 scheduling
//! validation, and the relay-chain validate-block entry. Also re-exports the shared
//! relay-chain primitive types so `relay_chain::X` paths across the pallet resolve here
//! (see the crate-root `relay_chain` binding in `lib.rs`).

pub mod descendant_validation;
pub mod events;
pub(crate) mod messaging;
#[cfg(not(feature = "std"))]
pub mod relay_chain_implementation;
pub mod relay_state_snapshot;
#[cfg(any(test, not(feature = "std")))]
pub mod scheduling;
mod segment;
pub mod state;
pub(crate) mod validation_data;

pub(crate) use segment::maybe_drop_included_ancestors;

// Re-exports of the shared relay-chain primitive types: the crate root binds `relay_chain` to
// this module, so every `relay_chain::X` path across the pallet resolves through these.
#[cfg(any(feature = "std", feature = "runtime-benchmarks", test))]
pub use cumulus_primitives_core::relay_chain::AsyncBackingParams;
pub use cumulus_primitives_core::relay_chain::{
	ApprovedPeerId, BlockNumber, Hash, UMPSignal, UpgradeGoAhead, UpgradeRestriction, UMP_SEPARATOR,
};
// Test-only re-exports used by the pallet's own `mock` and `tests` modules.
#[cfg(test)]
pub use cumulus_primitives_core::relay_chain::{HeadData, HrmpChannelId};

/// Something that can check the associated relay block number.
///
/// Each Parachain block is built in the context of a relay chain block, this trait allows us
/// to validate the given relay chain block number. With async backing it is legal to build
/// multiple Parachain blocks per relay chain parent. With this trait it is possible for the
/// Parachain to ensure that still only one Parachain block is build per relay chain parent.
///
/// By default [`RelayNumberStrictlyIncreases`] and [`AnyRelayNumber`] are provided.
pub trait CheckAssociatedRelayNumber {
	/// Check the current relay number versus the previous relay number.
	///
	/// The implementation should panic when there is something wrong.
	fn check_associated_relay_number(current: BlockNumber, previous: BlockNumber);
}

/// Provides an implementation of [`CheckAssociatedRelayNumber`].
///
/// It will ensure that the associated relay block number strictly increases between Parachain
/// blocks. This should be used by production Parachains when in doubt.
pub struct RelayNumberStrictlyIncreases;

impl CheckAssociatedRelayNumber for RelayNumberStrictlyIncreases {
	fn check_associated_relay_number(current: BlockNumber, previous: BlockNumber) {
		if current <= previous {
			panic!("Relay chain block number needs to strictly increase between Parachain blocks!")
		}
	}
}

/// Provides an implementation of [`CheckAssociatedRelayNumber`].
///
/// This will accept any relay chain block number combination. This is mainly useful for
/// test parachains.
pub struct AnyRelayNumber;

impl CheckAssociatedRelayNumber for AnyRelayNumber {
	fn check_associated_relay_number(_: BlockNumber, _: BlockNumber) {}
}

/// Provides an implementation of [`CheckAssociatedRelayNumber`].
///
/// It will ensure that the associated relay block number monotonically increases between Parachain
/// blocks. This should be used when asynchronous backing is enabled.
pub struct RelayNumberMonotonicallyIncreases;

impl CheckAssociatedRelayNumber for RelayNumberMonotonicallyIncreases {
	fn check_associated_relay_number(current: BlockNumber, previous: BlockNumber) {
		if current < previous {
			panic!(
				"Relay chain block number needs to monotonically increase between Parachain blocks!"
			)
		}
	}
}
