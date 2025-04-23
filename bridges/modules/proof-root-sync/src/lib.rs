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

//! A pallet for scheduling and syncing key-value pairs (roots) with arbitrary destinations.
//!
//! The pallet provides functionality to:
//! - Schedule roots for syncing using `schedule_for_sync`
//! - Automatically process scheduled roots during `on_idle` hooks
//!
//! The actual sending/syncing of roots is implemented by the `OnSend` trait, which can be customized
//! for specific use cases like cross-chain communication.
//!
//! Basically, this is a simple `on_idle` hook that can schedule data with a ring buffer and send data.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use frame_support::pallet_prelude::Weight;

pub mod impls;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;

const LOG_TARGET: &str = "runtime::bridge-proof-root-sync";

/// A trait for sending/syncing roots, for example, to other chains.
pub trait OnSend<Key, Value> {
	/// Process a list of roots (key-value pairs) for sending.
	///
	/// # Arguments
	///
	/// * `roots` - A vector of roots where each root is a tuple of (key, value).
	///    		Roots are ordered from the oldest (index 0) to the newest (last index).
	fn on_send(roots: &Vec<(Key, Value)>);

	/// Returns the weight consumed by `on_send`.
	fn on_send_weight() -> Weight;
}

#[impl_trait_for_tuples::impl_for_tuples(8)]
impl<Key, Value> OnSend<Key, Value> for Tuple {
	fn on_send(roots: &Vec<(Key, Value)>) {
		for_tuples!( #( Tuple::on_send(roots);) * );
	}

	fn on_send_weight() -> Weight {
		let mut weight: Weight = Default::default();
		for_tuples!( #( weight.saturating_accrue(Tuple::on_send_weight()); )* );
		weight
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::sp_runtime::SaturatedConversion;
	use frame_support::{pallet_prelude::*, weights::WeightMeter};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	/// The pallet configuration trait.
	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The key type used to identify stored values of type `T::Value`.
		type Key: Parameter;

		/// The type of the root value.
		type Value: Parameter;

		/// Maximum number of roots to retain in `RootsToSend` storage.
		/// This setting prevents unbounded growth of the on-chain state.
		/// If we hit this number, we start removing the oldest data from `RootsToSend`.
		#[pallet::constant]
		type RootsToKeep: Get<u32>;

		/// Maximum number of roots to drain and send with `T::OnSend`.
		#[pallet::constant]
		type MaxRootsToSend: Get<u32>;

		/// Means for sending/syncing roots.
		type OnSend: OnSend<Self::Key, Self::Value>;
	}

	/// A ring-buffer storage of roots (key-value pairs) that need to be sent/synced to other chains.
	/// When the buffer reaches its capacity limit defined by `T::RootsToKeep`, the oldest elements are removed.
	/// The elements are drained and processed in order by `T::OnSend` during `on_idle` up to `T::MaxRootsToSend` elements at a time.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type RootsToSend<T: Config<I>, I: 'static = ()> =
		StorageValue<_, VecDeque<(T::Key, T::Value)>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_idle(_n: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(limit);
			if meter.try_consume(Self::on_idle_weight()).is_err() {
				tracing::debug!(
					target: LOG_TARGET,
					?limit,
					on_idle_weight = ?Self::on_idle_weight(),
					"Not enough weight for on_idle.",
				);
				return meter.consumed();
			}

			// Send roots.
			RootsToSend::<T, I>::mutate(|roots| {
				let range_for_send =
					0..core::cmp::min(T::MaxRootsToSend::get().saturated_into(), roots.len());
				T::OnSend::on_send(&roots.drain(range_for_send).collect::<Vec<_>>())
			});

			meter.consumed()
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// The worst-case weight of [`Self::on_idle`].
		fn on_idle_weight() -> Weight {
			T::DbWeight::get()
				.reads_writes(1, 1)
				.saturating_add(T::OnSend::on_send_weight())
		}

		/// Schedule new data to be synced by `T::OnSend` means.
		///
		/// The roots are stored in a ring buffer with limited capacity as defined by `T::RootsToKeep`.
		/// When the buffer reaches its capacity limit, the oldest elements are removed.
		/// The elements will be drained and processed in order by `T::OnSend` during `on_idle` up to
		/// `T::MaxRootsToSend` elements at a time.
		pub fn schedule_for_sync(key: T::Key, value: T::Value) {
			RootsToSend::<T, I>::mutate(|roots| {
				// Add to schedules.
				roots.push_back((key, value));

				// Remove from the front up to the `T::RootsToKeep` limit.
				let max = T::RootsToKeep::get();
				while roots.len() > (max as usize) {
					let _ = roots.pop_front();
				}
			});
		}
	}
}
