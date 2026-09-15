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

//! Types that expose the relay chain state to other runtime modules.

use codec::{Decode, Encode};
use cumulus_primitives_core::PersistedValidationData;
use scale_info::TypeInfo;
use sp_runtime::traits::BlockNumberProvider;

use crate::{
	relay_chain::{BlockNumber, Hash},
	Config, Pallet, ValidationData,
};

/// Holds the most recent relay-parent state root and block number of the current parachain block.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Default, Debug)]
pub struct RelayChainState {
	/// Current relay chain height.
	pub number: BlockNumber,
	/// State root for current relay chain height.
	pub state_root: Hash,
}

/// This exposes the [`RelayChainState`] to other runtime modules.
///
/// Enables parachains to read relay chain state via state proofs.
pub trait RelaychainStateProvider {
	/// May be called by any runtime module to obtain the current state of the relay chain.
	///
	/// **NOTE**: This is not guaranteed to return monotonically increasing relay parents.
	fn current_relay_chain_state() -> RelayChainState;

	/// Utility function only to be used in benchmarking scenarios, to be implemented optionally,
	/// else a noop.
	///
	/// It allows for setting a custom RelayChainState.
	#[cfg(feature = "runtime-benchmarks")]
	fn set_current_relay_chain_state(_state: RelayChainState) {}
}

/// Implements [`BlockNumberProvider`] that returns relay chain block number fetched from validation
/// data.
///
/// When validation data is not available (e.g. within `on_initialize`), it will fallback to use
/// [`crate::Pallet::last_relay_block_number`].
///
/// Implements [`BlockNumberProvider`] and [`RelaychainStateProvider`] that returns relevant relay
/// data fetched from validation data.
///
/// NOTE: When validation data is not available (e.g. within `on_initialize`):
///
/// - [`current_relay_chain_state`](Self::current_relay_chain_state): Will return the default value
///   of [`RelayChainState`].
/// - [`current_block_number`](Self::current_block_number): Will return
///   [`crate::Pallet::last_relay_block_number`].
///
/// On JAM there is no relay chain and this type is absent: the relay block number is served by
/// the crate's `JamSlotNumber` (jam-only) instead.
#[cfg(not(jam))]
pub struct RelaychainDataProvider<T>(core::marker::PhantomData<T>);

#[cfg(not(jam))]
impl<T: Config> BlockNumberProvider for RelaychainDataProvider<T> {
	type BlockNumber = BlockNumber;

	fn current_block_number() -> BlockNumber {
		ValidationData::<T>::get()
			.map(|d| d.relay_parent_number)
			.unwrap_or_else(|| Pallet::<T>::last_relay_block_number())
	}

	#[cfg(any(feature = "std", feature = "runtime-benchmarks", test))]
	fn set_block_number(block: Self::BlockNumber) {
		let mut validation_data = ValidationData::<T>::get().unwrap_or_else(||
			// PersistedValidationData does not impl default in non-std
			PersistedValidationData {
				parent_head: vec![].into(),
				relay_parent_number: Default::default(),
				max_pov_size: Default::default(),
				relay_parent_storage_root: Default::default(),
			});
		validation_data.relay_parent_number = block;
		ValidationData::<T>::put(validation_data)
	}
}

#[cfg(not(jam))]
impl<T: Config> RelaychainStateProvider for RelaychainDataProvider<T> {
	fn current_relay_chain_state() -> RelayChainState {
		ValidationData::<T>::get()
			.map(|d| RelayChainState {
				number: d.relay_parent_number,
				state_root: d.relay_parent_storage_root,
			})
			.unwrap_or_default()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_current_relay_chain_state(state: RelayChainState) {
		let mut validation_data = ValidationData::<T>::get().unwrap_or_else(||
			// PersistedValidationData does not impl default in non-std
			PersistedValidationData {
				parent_head: vec![].into(),
				relay_parent_number: Default::default(),
				max_pov_size: Default::default(),
				relay_parent_storage_root: Default::default(),
			});
		validation_data.relay_parent_number = state.number;
		validation_data.relay_parent_storage_root = state.state_root;
		ValidationData::<T>::put(validation_data)
	}
}
