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

//! This pallet provides mechanisms for tracking and updating the last known head of a remote chain.
//! It allows submitting new head data and determines if the chain is stalled based on a threshold.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	pallet_prelude::{Decode, DecodeWithMemTracking, Encode, TypeInfo},
	sp_runtime::Saturating,
};
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_primitives::HeadData;

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking {
	//! TODO: FAIL-CI
}
#[cfg(test)]
mod mock {
	//! TODO: FAIL-CI
}
#[cfg(test)]
mod tests {
	//! TODO: FAIL-CI
}

/// Struct representing the latest known head of a remote chain.
#[derive(Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, PartialEq, TypeInfo)]
pub struct KnownHead<RemoteBlockNumber, LocalBlockNumber> {
	/// The block number of the known head.
	pub block_number: RemoteBlockNumber,
	/// The data associated with the known head.
	pub head: HeadData,
	/// The local runtime's block number when the head was last updated.
	pub known_at: LocalBlockNumber,
}

/// Trait for determining if the head synchronization has stalled.
pub trait IsStalled {
	/// Returns `true` if the synchronization is considered stalled.
	fn is_stalled() -> bool {
		Self::stalled_head().is_some()
	}

	/// Returns stalled `Some(head)` if the synchronization is considered stalled.
	fn stalled_head() -> Option<HeadData>;
}

/// An alias for `KnownHead` type.
pub type KnownHeadOf<T, I> =
	KnownHead<<T as pallet::Config<I>>::RemoteBlockNumber, BlockNumberFor<T>>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Configuration trait for the pallet.
	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// Event type for this pallet.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Weight information for dispatchable functions.
		type WeightInfo: WeightInfo;
		/// The origin permitted to submit head updates.
		type SubmitOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// The type representing remote block numbers.
		type RemoteBlockNumber: Parameter + Copy + Ord;
		/// The threshold for determining if synchronization has stalled.
		#[pallet::constant]
		type StalledThreshold: Get<BlockNumberFor<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// Events emitted by this pallet.
	#[pallet::event]
	#[pallet::generate_deposit(pub fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A new known head was recorded.
		NewHead { remote_block_number: T::RemoteBlockNumber },
	}

	/// Storage for the last known head of the remote chain.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type LastKnownHead<T: Config<I>, I: 'static = ()> =
		StorageValue<_, KnownHeadOf<T, I>, OptionQuery>;

	#[pallet::call(weight(<T as Config<I>>::WeightInfo))]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Records a new known head if it is more recent than the stored one.
		#[pallet::call_index(0)]
		pub fn note_new_head(
			origin: OriginFor<T>,
			remote_block_number: T::RemoteBlockNumber,
			remote_head: HeadData,
		) -> DispatchResult {
			let _ = T::SubmitOrigin::ensure_origin(origin);
			if Self::do_note_new_head(remote_block_number, remote_head) {
				Self::deposit_event(Event::NewHead { remote_block_number });
			}
			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Function to update the known head if it is newer (>=) than the stored one.
		/// Returns `true` if the update was successful.
		pub fn do_note_new_head(
			remote_block_number: T::RemoteBlockNumber,
			remote_head: HeadData,
		) -> bool {
			LastKnownHead::<T, I>::mutate(|last| match last {
				Some(head) if remote_block_number < head.block_number => {
					// The remote head is older, so do nothing.
					false
				},
				_ => {
					*last = Some(KnownHead {
						block_number: remote_block_number,
						head: remote_head,
						known_at: Self::now(),
					});
					true
				},
			})
		}

		/// Returns the current block number in the runtime.
		fn now() -> BlockNumberFor<T> {
			frame_system::Pallet::<T>::block_number()
		}

		/// Returns last known/synced head
		pub fn last_known_head() -> Option<KnownHeadOf<T, I>> {
			LastKnownHead::<T, I>::get()
		}
	}

	impl<T: Config<I>, I: 'static> IsStalled for Pallet<T, I> {
		fn stalled_head() -> Option<HeadData> {
			match LastKnownHead::<T, I>::get() {
				Some(head) => {
					let threshold = Self::now().saturating_sub(T::StalledThreshold::get());
					if head.known_at < threshold {
						Some(head.head)
					} else {
						None
					}
				},
				None => None,
			}
		}
	}
}
