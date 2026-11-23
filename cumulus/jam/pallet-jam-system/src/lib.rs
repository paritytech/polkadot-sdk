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

//! # JAM System Pallet
//!
//! The base pallet of a parachain backed by JAM (the Parachain Service) instead of a relay chain,
//! in place of `cumulus-pallet-parachain-system`. A JAM block carries no parachain inherent: its
//! context is the `JamParent` pre-runtime digest, the JAM state proof carried in the block's
//! additional data, and the timestamp inherent.
//!
//! The pallet owns:
//!
//! - the unincluded segment, pruned by the head the service records as included in the para's
//!   `ParaInfo` and bounded by the runtime's [`ConsensusHook`];
//! - the code upgrade by hash ([`Pallet::schedule_code_upgrade_hash`]);
//! - the service's feedback ([`Pallet::rejected_cores`], [`Pallet::latest_feedback`]) and the
//!   upward sender ([`Pallet::send_upward_message`]);
//! - the slot provider [`JamSlotNumber`];
//! - the `jam_validate_block` entry point the service's refine calls, exported by
//!   `register_jam_validate_block!` (JAM builds only).
//!
//! The validation core and the dynamic block weight logic come from
//! `cumulus-pallet-parachain-system`. A JAM runtime implements its `block_weight::Config` itself,
//! with [`Pallet::block_weight_mode`], [`Pallet::set_block_weight_mode`] and
//! [`Pallet::previous_core_count`].
//!
//! ## Per block
//!
//! `on_initialize` hands the block's `JamParent` digest to [`ConsensusHook::on_block`], reads the
//! para's `ParaInfo` once ([`Pallet::para_info`]), updates the unincluded segment from it, and
//! reconciles a pending code upgrade with it.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod feedback;
#[cfg(jam)]
mod host_overrides;
#[cfg(jam)]
mod macros;
#[cfg(test)]
mod mock;
#[cfg(any(jam, test))]
mod proof_roots;
pub mod slot;
#[cfg(test)]
mod tests;
mod upgrade;
mod upward;
#[cfg(jam)]
mod validate_block;

/// What the `register_jam_validate_block!` expansion names through this crate.
#[cfg(jam)]
#[doc(hidden)]
pub mod __private {
	pub use crate::validate_block::jam_validate_block;
	pub use cumulus_pallet_parachain_system::validate_block::{
		sp_api::__private::{polkavm_abi, polkavm_export},
		GetRuntimeBlockType,
	};
}

use alloc::vec::Vec;
use codec::{Compact, Decode, DecodeAll};
use core::{marker::PhantomData, num::NonZeroU32};
use cumulus_pallet_parachain_system::block_weight;
use cumulus_primitives_core::{CumulusDigestItem, JamParent, ParaId};
use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use frame_system::pallet_prelude::*;
use parachain_service_core::ParaInfo;
use sp_runtime::traits::{One, Zero};

pub use pallet::*;
pub use slot::JamSlotNumber;
pub use upgrade::EmitPhase;

/// Bounds the unincluded segment of a JAM parachain.
///
/// [`Pallet`] calls it at the start of every block's `on_initialize`, before it updates the
/// segment.
pub trait ConsensusHook {
	/// Check the block against the hook's rules, given the JAM anchors its `JamParent` digest
	/// names, and return the weight consumed and the capacity of the unincluded segment.
	///
	/// Panics if the block breaks the rules.
	fn on_block(parent: &JamParent) -> (Weight, NonZeroU32);
}

/// Called in the block that applies a scheduled code upgrade.
pub trait OnCodeApplied {
	/// The block writes the new code to `:code`; it runs from the next block on.
	fn on_code_applied();
}

impl OnCodeApplied for () {
	fn on_code_applied() {}
}

/// The [`frame_system::SetCode`] of a JAM runtime: it accepts no code bytes.
///
/// A JAM runtime upgrades by hash, through [`Pallet::schedule_code_upgrade_hash`]: the service
/// serves the code, and an extrinsic carrying multi-MB code bytes traps the PolkaVM runtime.
pub struct JamSetCode<T>(PhantomData<T>);

impl<T: Config> frame_system::SetCode<T> for JamSetCode<T> {
	fn set_code(_code: Vec<u8>) -> DispatchResult {
		Err(Error::<T>::CodeBytesNotAccepted.into())
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// The id of this parachain.
		#[pallet::constant]
		type SelfParaId: Get<ParaId>;

		/// Checks every block and bounds the unincluded segment.
		type ConsensusHook: ConsensusHook;

		/// Called in the block that applies a scheduled code upgrade.
		type OnCodeApplied: OnCodeApplied;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			// Both belong to the block that writes them: `jam_validate_block` reads the emission
			// right after this block executes, and the weight mode is set by this block's
			// extrinsics.
			PendingUpgradeEmit::<T>::kill();
			BlockWeightMode::<T>::kill();

			let parent = slot::jam_parent::<T>()
				.expect("a JAM block carries a JamParent digest; refine rejects one without; qed");
			let (hook_weight, capacity) = T::ConsensusHook::on_block(&parent);

			let para_info = Self::para_info();
			Self::update_unincluded_segment(n, para_info.as_ref(), capacity);
			upgrade::apply_if_ready_now::<T>(para_info.as_ref());

			// Reads: the segment and the pending upgrade. Writes: the two kills above, the
			// segment, an applied upgrade (`:code`, `PendingCodeHash`), the emission and
			// `PreviousCoreCount` in `on_finalize`.
			hook_weight.saturating_add(T::DbWeight::get().reads_writes(2, 7))
		}

		fn on_finalize(_: BlockNumberFor<T>) {
			// A block without `CoreInfo` was built for one core, as the shared block weight logic
			// counts it.
			let core_info = CumulusDigestItem::find_core_info(&frame_system::Pallet::<T>::digest());
			PreviousCoreCount::<T>::put(core_info.map_or(Compact(1), |info| info.number_of_cores));
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Schedule a code upgrade by the code's blake2b-256 hash and byte length.
		///
		/// Every block reconciles the pending upgrade with the service: the runtime announces the
		/// code until the service has seen it, then applies it by writing the 40-byte code marker
		/// to `:code`. The code bytes never travel in an extrinsic; a multi-MB extrinsic traps the
		/// PolkaVM runtime.
		#[pallet::call_index(0)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn schedule_code_upgrade_hash(
			origin: OriginFor<T>,
			code_hash: [u8; 32],
			code_len: u32,
		) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(!PendingCodeHash::<T>::exists(), Error::<T>::OverlappingUpgrades);
			ensure!(upgrade::code_size_fits(code_len as usize), Error::<T>::TooBig);
			PendingCodeHash::<T>::put((code_hash, code_len));
			Self::deposit_event(Event::ValidationFunctionStored);
			Ok(())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A code upgrade was scheduled by hash.
		ValidationFunctionStored,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A code upgrade is already pending.
		OverlappingUpgrades,
		/// The code is larger than the largest preimage JAM carries.
		TooBig,
		/// Code bytes were passed to `set_code`; a JAM runtime upgrades by hash through
		/// `schedule_code_upgrade_hash`.
		CodeBytesNotAccepted,
	}

	/// The hashes of the unincluded ancestors of the current block, oldest first.
	///
	/// The current block has no entry: its hash is unknown while it executes.
	#[pallet::storage]
	pub type UnincludedSegment<T: Config> = StorageValue<_, Vec<T::Hash>, ValueQuery>;

	/// The pending code upgrade: blake2b-256 hash and byte length of the new code.
	///
	/// Set by [`Pallet::schedule_code_upgrade_hash`], cleared when the code is applied.
	#[pallet::storage]
	pub type PendingCodeHash<T: Config> = StorageValue<_, ([u8; 32], u32), OptionQuery>;

	/// The code-upgrade upward message this block emits, as `(hash, len, phase)`.
	///
	/// Written while the block executes and sent by `jam_validate_block` after execution, the
	/// only place the service host calls exist. Cleared at the start of the next block.
	#[pallet::storage]
	pub type PendingUpgradeEmit<T: Config> =
		StorageValue<_, ([u8; 32], u32, EmitPhase), OptionQuery>;

	/// The current block weight mode of the dynamic block weight logic.
	///
	/// Killed in `on_initialize`, set through the runtime's `block_weight::Config`.
	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub type BlockWeightMode<T: Config> =
		StorageValue<_, block_weight::BlockWeightMode<T>, OptionQuery>;

	/// The number of cores the previous block was built for.
	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub type PreviousCoreCount<T: Config> = StorageValue<_, Compact<u16>, OptionQuery>;
}

impl<T: Config> Pallet<T> {
	/// This parachain's `ParaInfo` as the Parachain Service records it, read from the JAM state
	/// proof carried with the block.
	///
	/// `None` when the service holds no `ParaInfo` for the para, as before its registration, or a
	/// value that does not decode. A proof that cannot authenticate the key panics in the read
	/// rather than yield `None`.
	pub fn para_info() -> Option<ParaInfo> {
		let para_id = parachain_service_core::types::ParaId::from(u32::from(T::SelfParaId::get()));
		let key = parachain_service_core::service_value_state_key(
			parachain_service_core::PARACHAIN_SERVICE_ID,
			&parachain_service_core::para_info_key(para_id),
		);
		cumulus_jam_state_reader::jam_state::jam_state_read(key)
			.and_then(|raw| ParaInfo::decode(&mut &raw[..]).ok())
	}

	/// The number of unincluded blocks after `included`, counting the block this state belongs
	/// to.
	///
	/// [`UnincludedSegment`] has no entry for the block itself, so it counts one more, like the
	/// hash-less newest ancestor of the relay pallet's segment. A hash not in the segment counts
	/// the whole segment. Zero at genesis, which has no unincluded block.
	pub fn unincluded_segment_size_after(included: T::Hash) -> u32 {
		if frame_system::Pallet::<T>::block_number().is_zero() {
			return 0;
		}
		let segment = UnincludedSegment::<T>::get();
		let after = segment
			.iter()
			.position(|hash| *hash == included)
			.map_or(segment.len(), |position| segment.len() - position - 1);
		after as u32 + 1
	}

	/// The current [`block_weight::BlockWeightMode`], for the runtime's `block_weight::Config`.
	pub fn block_weight_mode() -> Option<block_weight::BlockWeightMode<T>> {
		BlockWeightMode::<T>::get()
	}

	/// Replace the current [`block_weight::BlockWeightMode`]; `None` removes it. For the runtime's
	/// `block_weight::Config`.
	pub fn set_block_weight_mode(mode: Option<block_weight::BlockWeightMode<T>>) {
		BlockWeightMode::<T>::set(mode)
	}

	/// The number of cores the previous block was built for, for the runtime's
	/// `block_weight::Config`.
	pub fn previous_core_count() -> Option<u16> {
		PreviousCoreCount::<T>::get().map(|count| count.0)
	}

	/// Push block `n`'s parent onto the unincluded segment and drop what the service included.
	///
	/// Block 1 builds on genesis, which is included by definition, so parents enter the segment
	/// from block 2 on. The service records the included block's header hash as its head data;
	/// the segment is drained up to and including that hash. An absent `ParaInfo`, or a head
	/// that is not in the segment, drains nothing.
	///
	/// Panics when the segment leaves no space for block `n` within `capacity`.
	fn update_unincluded_segment(
		n: BlockNumberFor<T>,
		para_info: Option<&ParaInfo>,
		capacity: NonZeroU32,
	) {
		let included =
			para_info.and_then(|info| T::Hash::decode_all(&mut &info.head_data[..]).ok());
		UnincludedSegment::<T>::mutate(|segment| {
			if n > One::one() {
				segment.push(frame_system::Pallet::<T>::parent_hash());
			}
			if let Some(position) =
				included.and_then(|head| segment.iter().position(|hash| *hash == head))
			{
				segment.drain(..=position);
			}
			assert!(
				segment.len() < capacity.get() as usize,
				"No space left for the block in the unincluded segment: len({}) < capacity({})",
				segment.len(),
				capacity,
			);
		});
	}
}
