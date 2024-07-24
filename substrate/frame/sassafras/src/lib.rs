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

//! Extension module for Sassafras consensus.
//!
//! [Sassafras](https://research.web3.foundation/Polkadot/protocols/block-production/SASSAFRAS)
//! is a constant-time block production protocol that aims to ensure that there is
//! exactly one block produced with constant time intervals rather than multiple or none.
//!
//! We run a lottery to distribute block production slots for a *target* epoch and to fix
//! the order validators produce blocks in.
//!
//! Each validator signs some unbiasable VRF input and publishes the VRF output on-chain.
//! This value is their lottery ticket that can be eventually validated against their
//! public key.
//!
//! We want to keep lottery winners secret, i.e. do not disclose their public keys.
//! At the beginning of the *target* epoch all the validators tickets are published but
//! not the corresponding author public keys.
//!
//! The association is revealed by the ticket's owner during block production when he will
//! claim his ticket, and thus the associated slot, by showing a proof which ships with the
//! produced block.
//!
//! To prevent submission of invalid tickets, resulting in empty slots, the validator
//! when submitting a ticket accompanies it with a zk-SNARK of the statement:
//! "Here's my VRF output that has been generated using the given VRF input and my secret
//! key. I'm not telling you who I am, but my public key is among those of the nominated
//! validators for the target epoch".

#![allow(unused)]
#![deny(warnings)]
#![warn(unused_must_use, unsafe_code, unused_variables, unused_imports, missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, Encode, MaxEncodedLen};
use log::{debug, error, trace, warn};
use scale_info::TypeInfo;

use alloc::vec::Vec;
use frame_support::{
	dispatch::DispatchResult,
	traits::{ConstU32, Get},
	weights::Weight,
	BoundedVec, WeakBoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_consensus_sassafras::{
	digests::{ConsensusLog, NextEpochDescriptor, SlotClaim},
	vrf, AuthorityId, Configuration, Epoch, InherentError, InherentType, Randomness, Slot,
	TicketBody, TicketEnvelope, TicketId, INHERENT_IDENTIFIER, RANDOMNESS_LENGTH,
	SASSAFRAS_ENGINE_ID,
};
use sp_io::hashing;
use sp_runtime::{
	generic::DigestItem,
	traits::{One, Zero},
	BoundToRuntimeAppPublic,
};

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(all(feature = "std", test))]
mod mock;
#[cfg(all(feature = "std", test))]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

const LOG_TARGET: &str = "sassafras::runtime";

// Contextual string used by the VRF to generate per-block randomness.
const RANDOMNESS_VRF_CONTEXT: &[u8] = b"SassafrasOnChainRandomness";

// Epoch tail is the section of the epoch where no tickets are allowed to be submitted.
// As the name implies, this section is at the end of an epoch.
//
// Length of the epoch's tail is computed as `Config::EpochLength / EPOCH_TAIL_FRACTION`
// TODO: make this part of `Config`?
const EPOCH_TAIL_FRACTION: u32 = 6;

/// Max number of tickets that can be submitted in one block.
// TODO: make this part of `Config`?
const TICKETS_CHUNK_MAX_LENGTH: u32 = 16;

/// Randomness buffer.
pub type RandomnessBuffer = [Randomness; 4];

/// Number of tickets available for current and next epoch.
///
/// These tickets are held by the [`Tickets`] storage map.
///
/// Current counter index is computed as current epoch index modulo 2
/// Next counter index is computed as the other entry.
pub type TicketsCounter = [u32; 2];

/// Ephemeral data constructed by `on_initialize` and destroyed by `on_finalize`.
///
/// Contains some temporary data that may be useful later during code execution.
#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct EphemeralData {
	/// Previous block slot.
	prev_slot: Slot,
	/// Per block randomness to be deposited after block execution (on finalization).
	block_randomness: Randomness,
}

/// Key used for the tickets accumulator map.
///
/// Ticket keys are constructed by taking the bitwise negation of the ticket identifier.
/// As the tickets accumulator sorts entries according to the key values from smaller
/// to larger, we end up with a sequence of tickets identifiers sorted from larger to
/// smaller.
///
/// This strategy comes handy when we quickly need to check if a new ticket chunk has been
/// completely absorbed by the accumulator, when this is already full and without loading
/// the whole sequence in memory.
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
pub struct TicketKey([u8; 32]);

impl From<TicketId> for TicketKey {
	fn from(mut value: TicketId) -> Self {
		TicketKey(value.0.map(|b| !b))
	}
}

/// Authorities sequence.
pub type AuthoritiesVec<T> = WeakBoundedVec<AuthorityId, <T as Config>::MaxAuthorities>;

/// Tickets sequence.
pub type TicketsVec = BoundedVec<TicketEnvelope, ConstU32<TICKETS_CHUNK_MAX_LENGTH>>;

trait EpochTag {
	fn tag(&self) -> u8;
	fn next_tag(&self) -> u8;
}

impl EpochTag for u64 {
	#[inline(always)]
	fn tag(&self) -> u8 {
		(self % 2) as u8
	}

	#[inline(always)]
	fn next_tag(&self) -> u8 {
		self.tag() ^ 1
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// The Sassafras pallet.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration parameters.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Amount of slots that each epoch should last.
		#[pallet::constant]
		type EpochLength: Get<u32>;

		/// Max number of authorities allowed.
		#[pallet::constant]
		type MaxAuthorities: Get<u32>;

		/// Redundancy factor
		#[pallet::constant]
		type RedundancyFactor: Get<u8>;

		/// Max attempts number
		#[pallet::constant]
		type AttemptsNumber: Get<u8>;

		/// Epoch change trigger.
		///
		/// Logic to be triggered on every block to query for whether an epoch has ended
		/// and to perform the transition to the next epoch.
		type EpochChangeTrigger: EpochChangeTrigger;

		/// Weight information for all calls of this pallet.
		type WeightInfo: weights::WeightInfo;
	}

	/// Sassafras runtime errors.
	#[pallet::error]
	pub enum Error<T> {
		/// Ticket identifier is too big.
		TicketOverThreshold,
		/// Duplicate ticket
		TicketDuplicate,
		/// Bad ticket order
		TicketBadOrder,
		/// Invalid ticket
		TicketInvalid,
		/// Invalid ticket signature
		TicketBadProof,
	}

	/// Current epoch authorities.
	#[pallet::storage]
	#[pallet::getter(fn authorities)]
	pub type Authorities<T: Config> = StorageValue<_, AuthoritiesVec<T>, ValueQuery>;

	/// Next epoch authorities.
	#[pallet::storage]
	#[pallet::getter(fn next_authorities)]
	pub type NextAuthorities<T: Config> = StorageValue<_, AuthoritiesVec<T>, ValueQuery>;

	/// Current block slot number.
	#[pallet::storage]
	#[pallet::getter(fn current_slot)]
	pub type CurrentSlot<T> = StorageValue<_, Slot, ValueQuery>;

	/// Randomness buffer.
	#[pallet::storage]
	#[pallet::getter(fn randomness_buf)]
	pub type RandomnessBuf<T> = StorageValue<_, RandomnessBuffer, ValueQuery>;

	/// Tickets accumulator.
	#[pallet::storage]
	#[pallet::getter(fn tickets_accumulator)]
	pub type TicketsAccumulator<T> = CountedStorageMap<_, Identity, TicketKey, TicketBody>;

	/// Tickets counters for the current and next epoch.
	#[pallet::storage]
	#[pallet::getter(fn tickets_count)]
	pub type TicketsCount<T> = StorageValue<_, TicketsCounter, ValueQuery>;

	/// Tickets map.
	///
	/// The map holds tickets identifiers for the current and next epoch.
	///
	/// The key is a tuple composed by:
	/// - `u8`: equal to epoch's index modulo 2;
	/// - `u32` equal to the ticket's index in an abstract sorted sequence of epoch's tickets.
	///
	/// For example, the key for the `N`-th ticket for epoch `E` is `(E mod 2, N)`
	///
	/// Note that the ticket's index `N` doesn't correspond to the offset of the associated
	/// slot within the epoch. The assignment is computed using an *outside-in* strategy
	/// and correctly returned by the [`slot_ticket`] method.
	///
	/// Be aware that entries within this map are never removed, but only overwritten.
	/// The number of tickets available for epoch `E` is stored in the `E mod 2` entry
	/// of [`TicketsCount`].
	#[pallet::storage]
	#[pallet::getter(fn tickets)]
	pub type Tickets<T> = StorageMap<_, Identity, (u8, u32), TicketBody>;

	/// Parameters used to construct the epoch's ring verifier.
	///
	/// In practice, this is the SNARK "Universal Reference String" (powers of tau).
	#[pallet::storage]
	#[pallet::getter(fn ring_context)]
	pub type RingContext<T: Config> = StorageValue<_, vrf::RingContext>;

	/// Ring verifier data for the current epoch.
	#[pallet::storage]
	#[pallet::getter(fn ring_verifier_key)]
	pub type RingVerifierKey<T: Config> = StorageValue<_, vrf::RingVerifierKey>;

	/// Ephemeral data we retain until the block finalization.
	#[pallet::storage]
	pub(crate) type TemporaryData<T> = StorageValue<_, EphemeralData>;

	/// Genesis configuration for Sassafras protocol.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Genesis authorities.
		pub authorities: Vec<AuthorityId>,
		/// Phantom config
		#[serde(skip)]
		pub _phantom: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			Pallet::<T>::genesis_authorities_initialize(&self.authorities);

			#[cfg(feature = "construct-dummy-ring-context")]
			{
				debug!(target: LOG_TARGET, "Constructing dummy ring context");
				let ring_ctx = vrf::RingContext::new_testing();
				RingContext::<T>::put(ring_ctx);
				Pallet::<T>::update_ring_verifier(&self.authorities);
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(block_num: BlockNumberFor<T>) -> Weight {
			debug_assert_eq!(block_num, frame_system::Pallet::<T>::block_number());

			let claim = <frame_system::Pallet<T>>::digest()
				.logs
				.iter()
				.find_map(|item| item.pre_runtime_try_to::<SlotClaim>(&SASSAFRAS_ENGINE_ID))
				.expect("Valid block must have a slot claim. qed");

			let randomness_accumulator = Self::randomness_accumulator();
			let randomness_input = vrf::block_randomness_input(&randomness_accumulator, claim.slot);

			// Verification has already been done by the host
			debug_assert!({
				use sp_core::crypto::{VrfPublic, Wraps};
				let authorities = Authorities::<T>::get();
				let public = authorities
					.get(claim.authority_idx as usize)
					.expect("Bad authority index in claim");
				let data = vrf::block_randomness_sign_data(&randomness_accumulator, claim.slot);
				public.as_inner_ref().vrf_verify(&data, &claim.vrf_signature)
			});

			let block_randomness = claim.vrf_signature.pre_outputs[0]
				.make_bytes::<RANDOMNESS_LENGTH>(RANDOMNESS_VRF_CONTEXT, &randomness_input);

			TemporaryData::<T>::put(EphemeralData {
				prev_slot: CurrentSlot::<T>::get(),
				block_randomness,
			});

			CurrentSlot::<T>::put(claim.slot);

			if block_num == One::one() {
				Self::post_genesis_initialize();
			}

			let trigger_weight = T::EpochChangeTrigger::trigger::<T>(block_num);

			T::WeightInfo::on_initialize() + trigger_weight
		}

		fn on_finalize(_: BlockNumberFor<T>) {
			// At the end of the block, we can safely include the current block randomness
			// to the accumulator. If we've determined that this block was the first in
			// a new epoch, the changeover logic has already occurred at this point
			// (i.e. `enact_epoch_change` has already been called).
			let block_randomness = TemporaryData::<T>::take()
				.expect("Unconditionally populated in `on_initialize`; `on_finalize` is always called after; qed")
				.block_randomness;
			Self::deposit_randomness(block_randomness);

			// Check if we are in the epoch's tail.
			// If so, start sorting the next epoch tickets.
			let epoch_length = T::EpochLength::get();
			let current_slot_idx = Self::current_slot_index();
			let mut outstanding_count = TicketsAccumulator::<T>::count() as usize;
			if current_slot_idx >= epoch_length - epoch_length / EPOCH_TAIL_FRACTION &&
				outstanding_count != 0
			{
				let slots_left = epoch_length.checked_sub(current_slot_idx + 1).unwrap_or(1);
				if slots_left > 0 {
					outstanding_count = outstanding_count.div_ceil(slots_left as usize);
				}
				let next_epoch_tag = Self::current_epoch_index().next_tag();
				Self::consume_tickets_accumulator(outstanding_count, next_epoch_tag);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit next epoch tickets candidates.
		///
		/// The number of tickets allowed to be submitted in one call is equal to the epoch length.
		#[pallet::call_index(0)]
		#[pallet::weight((
			T::WeightInfo::submit_tickets(envelopes.len() as u32),
			DispatchClass::Mandatory
		))]
		pub fn submit_tickets(origin: OriginFor<T>, envelopes: TicketsVec) -> DispatchResult {
			ensure_none(origin)?;

			debug!(target: LOG_TARGET, "Received {} tickets", envelopes.len());

			let epoch_length = T::EpochLength::get();
			let current_slot_idx = Self::current_slot_index();
			if current_slot_idx > epoch_length / 2 {
				warn!(target: LOG_TARGET, "Tickets shall be submitted in the first epoch half",);
				return Err("Tickets shall be submitted in the first epoch half".into())
			}

			let Some(verifier) = RingVerifierKey::<T>::get().map(|v| v.into()) else {
				warn!(target: LOG_TARGET, "Ring verifier key not initialized");
				return Err("Ring verifier key not initialized".into())
			};

			// Get next epoch parameters
			let randomness = Self::next_randomness();
			let authorities = Self::next_authorities();

			// Compute tickets threshold
			let ticket_threshold = sp_consensus_sassafras::ticket_id_threshold(
				epoch_length as u32,
				authorities.len() as u32,
				T::AttemptsNumber::get(),
				T::RedundancyFactor::get(),
			);

			let mut candidates = Vec::new();
			for envelope in envelopes {
				let Some(ticket_id_pre_output) = envelope.signature.pre_outputs.get(0) else {
					debug!(target: LOG_TARGET, "Missing ticket VRF pre-output from ring signature");
					return Err(Error::<T>::TicketInvalid.into())
				};
				let ticket_id_input = vrf::ticket_id_input(&randomness, envelope.attempt);

				// Check threshold constraint
				let ticket_id = vrf::make_ticket_id(&ticket_id_input, &ticket_id_pre_output);
				trace!(target: LOG_TARGET, "Checking ticket {:?}", ticket_id);

				if ticket_id >= ticket_threshold {
					debug!(target: LOG_TARGET, "Ticket over threshold ({:?} >= {:?})", ticket_id, ticket_threshold);
					return Err(Error::<T>::TicketOverThreshold.into())
				}

				// Check ring signature
				let sign_data = vrf::ticket_id_sign_data(ticket_id_input, &envelope.extra);
				if !envelope.signature.ring_vrf_verify(&sign_data, &verifier) {
					debug!(target: LOG_TARGET, "Proof verification failure for ticket ({:?})", ticket_id);
					return Err(Error::<T>::TicketBadProof.into())
				}

				candidates.push(TicketBody {
					id: ticket_id,
					attempt: envelope.attempt,
					extra: envelope.extra,
				});
			}

			Self::deposit_tickets(candidates)?;

			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let envelopes = data
				.get_data::<InherentType>(&INHERENT_IDENTIFIER)
				.expect("Sassafras inherent data not correctly encoded")
				.expect("Sassafras inherent data must be provided");

			let envelopes = BoundedVec::truncate_from(envelopes);
			Some(Call::submit_tickets { envelopes })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::submit_tickets { .. })
		}
	}
}

// Inherent methods
impl<T: Config> Pallet<T> {
	pub(crate) fn update_ring_verifier(authorities: &[AuthorityId]) {
		debug!(target: LOG_TARGET, "Loading ring context");
		let Some(ring_ctx) = RingContext::<T>::get() else {
			debug!(target: LOG_TARGET, "Ring context not initialized");
			return
		};

		let pks: Vec<_> = authorities.iter().map(|auth| *auth.as_ref()).collect();

		debug!(target: LOG_TARGET, "Building ring verifier (ring size: {})", pks.len());
		let maybe_verifier_key = ring_ctx.verifier_key(&pks);
		if maybe_verifier_key.is_none() {
			error!(
				target: LOG_TARGET,
				"Failed to build verifier key. This should never happen,\n
				 falling back to AURA for next epoch as last resort"
			);
		}
		RingVerifierKey::<T>::set(maybe_verifier_key);
	}

	/// Enact an epoch change.
	///
	/// WARNING: Should be called on every block once and if and only if [`should_end_epoch`]
	/// has returned `true`.
	///
	/// If we detect one or more skipped epochs the policy is to use the authorities and values
	/// from the first skipped epoch. The tickets data is invalidated.
	pub(crate) fn enact_epoch_change(
		authorities: WeakBoundedVec<AuthorityId, T::MaxAuthorities>,
		next_authorities: WeakBoundedVec<AuthorityId, T::MaxAuthorities>,
	) {
		debug_assert_eq!(authorities, NextAuthorities::<T>::get());

		if next_authorities != authorities {
			Self::update_ring_verifier(&next_authorities);
		}

		// Update authorities
		Authorities::<T>::put(&authorities);
		NextAuthorities::<T>::put(&next_authorities);

		// Update epoch index
		let expected_epoch_idx = TemporaryData::<T>::get()
			.map(|cache| Self::epoch_index(cache.prev_slot) + 1)
			.expect("Unconditionally populated in `on_initialize`; `enact_epoch_change` is always called after; qed");
		let mut epoch_idx = Self::current_epoch_index();

		if epoch_idx < expected_epoch_idx {
			panic!(
				"Unexpected epoch value, expected: {} - found: {}, aborting",
				expected_epoch_idx, epoch_idx
			);
		}

		if expected_epoch_idx != epoch_idx {
			// Detected one or more skipped epochs, clear tickets data and recompute epoch index.
			Self::reset_tickets_data();
			let skipped_epochs = epoch_idx - expected_epoch_idx;
			epoch_idx += skipped_epochs;
			warn!(
				target: LOG_TARGET,
				"Detected {} skipped epochs, resuming from epoch {}",
				skipped_epochs,
				epoch_idx
			);
		}

		// After we update the current epoch, we signal the *next* epoch change
		// so that nodes can track changes.
		let epoch_signal = NextEpochDescriptor {
			randomness: Self::update_randomness_buffer(),
			authorities: next_authorities.into_inner(),
		};
		Self::deposit_next_epoch_descriptor_digest(epoch_signal);

		Self::consume_tickets_accumulator(usize::MAX, epoch_idx.tag());

		// Reset next epoch counter as we're start accumulating.
		let mut tickets_count = TicketsCount::<T>::get();
		tickets_count[epoch_idx.next_tag() as usize] = 0;
		TicketsCount::<T>::set(tickets_count);
	}

	pub(crate) fn deposit_tickets(tickets: Vec<TicketBody>) -> Result<(), Error<T>> {
		let prev_count = TicketsAccumulator::<T>::count();
		let mut prev_id = None;
		for ticket in &tickets {
			if prev_id.map(|prev| ticket.id <= prev).unwrap_or_default() {
				return Err(Error::TicketBadOrder)
			}
			prev_id = Some(ticket.id);
			TicketsAccumulator::<T>::insert(TicketKey::from(ticket.id), ticket);
		}
		let count = TicketsAccumulator::<T>::count();
		if count != prev_count + tickets.len() as u32 {
			return Err(Error::TicketDuplicate)
		}
		let diff = count.saturating_sub(T::EpochLength::get());
		if diff > 0 {
			let dropped_entries: Vec<_> =
				TicketsAccumulator::<T>::iter().take(diff as usize).collect();
			// Assess that no new ticket has been dropped
			for (key, ticket) in dropped_entries {
				if tickets.binary_search_by_key(&ticket.id, |t| t.id).is_ok() {
					return Err(Error::TicketInvalid)
				}
				TicketsAccumulator::<T>::remove(key);
			}
		}
		Ok(())
	}

	fn consume_tickets_accumulator(max_items: usize, epoch_tag: u8) {
		let mut tickets_count = TicketsCount::<T>::get();
		let mut accumulator_count = TicketsAccumulator::<T>::count();
		let mut idx = accumulator_count;
		for (_, ticket) in TicketsAccumulator::<T>::drain().take(max_items) {
			idx -= 1;
			Tickets::<T>::insert((epoch_tag, idx), ticket);
		}
		tickets_count[epoch_tag as usize] += (accumulator_count - idx);
		TicketsCount::<T>::set(tickets_count);
	}

	// Call this function on epoch change to enact current epoch randomness.
	fn update_randomness_buffer() -> Randomness {
		let mut randomness = RandomnessBuf::<T>::get();
		randomness[3] = randomness[2];
		randomness[2] = randomness[1];
		randomness[1] = randomness[0];
		let announce = randomness[2];
		RandomnessBuf::<T>::put(randomness);
		announce
	}

	// Deposit per-slot randomness.
	fn deposit_randomness(randomness: Randomness) {
		let mut accumulator = RandomnessBuf::<T>::get();
		let mut buf = [0; 2 * RANDOMNESS_LENGTH];
		buf[..RANDOMNESS_LENGTH].copy_from_slice(&accumulator[0][..]);
		buf[RANDOMNESS_LENGTH..].copy_from_slice(&randomness[..]);
		accumulator[0] = hashing::blake2_256(&buf);
		RandomnessBuf::<T>::put(accumulator);
	}

	// Deposit next epoch descriptor in the block header digest.
	fn deposit_next_epoch_descriptor_digest(desc: NextEpochDescriptor) {
		let item = ConsensusLog::NextEpochData(desc);
		let log = DigestItem::Consensus(SASSAFRAS_ENGINE_ID, item.encode());
		<frame_system::Pallet<T>>::deposit_log(log)
	}

	// Initialize authorities on genesis phase.
	//
	// Genesis authorities may have been initialized via other means (e.g. via session pallet).
	//
	// If this function has already been called with some authorities, then the new list
	// should match the previously set one.
	fn genesis_authorities_initialize(authorities: &[AuthorityId]) {
		let prev_authorities = Authorities::<T>::get();

		if !prev_authorities.is_empty() {
			// This function has already been called.
			if prev_authorities.as_slice() == authorities {
				return
			} else {
				panic!("Authorities were already initialized");
			}
		}

		let authorities = WeakBoundedVec::try_from(authorities.to_vec())
			.expect("Initial number of authorities should be lower than T::MaxAuthorities");
		Authorities::<T>::put(&authorities);
		NextAuthorities::<T>::put(&authorities);
	}

	// Method to be called on first block `on_initialize` to properly populate some key parameters.
	fn post_genesis_initialize() {
		// Properly initialize randomness using genesis hash.
		// This is important to guarantee that a different set of tickets are produced for
		// different chains sharing the same ring parameters.
		let genesis_hash = frame_system::Pallet::<T>::parent_hash();
		let mut accumulator = RandomnessBuffer::default();
		accumulator[0] = hashing::blake2_256(genesis_hash.as_ref());
		accumulator[1] = hashing::blake2_256(&accumulator[0]);
		accumulator[2] = hashing::blake2_256(&accumulator[1]);
		accumulator[3] = hashing::blake2_256(&accumulator[2]);
		RandomnessBuf::<T>::put(accumulator);

		// Deposit a log as this is the first block in first epoch.
		let next_epoch = NextEpochDescriptor {
			randomness: accumulator[2],
			authorities: Self::next_authorities().into_inner(),
		};
		Self::deposit_next_epoch_descriptor_digest(next_epoch);
	}

	/// Fetch expected ticket-id for the given slot according to an "outside-in" sorting strategy.
	///
	/// Given an ordered sequence of tickets [t0, t1, t2, ..., tk] to be assigned to n slots,
	/// with n >= k, then the tickets are assigned to the slots according to the following
	/// strategy:
	///
	/// slot-index  : [ 0,  1,  2,  3,  .................. ,n ]
	/// tickets     : [ t1, tk, t2, t_{k-1} ..... ].
	///
	/// With slot-index computed as `epoch_start() - slot`.
	///
	/// If `slot` value falls within the current epoch then we fetch tickets from the current epoch
	/// tickets list.
	///
	/// If `slot` value falls within the next epoch then we fetch tickets from the next epoch
	/// tickets ids list. Note that in this case we may have not finished receiving all the tickets
	/// for that epoch yet. The next epoch tickets should be considered "stable" only after the
	/// current epoch "submission period" is completed.
	///
	/// Returns `None` if, according to the sorting strategy, there is no ticket associated to the
	/// specified slot-index (may happen if n > k and we are requesting for a ticket for a slot with
	/// relative index i > k) or if the slot falls beyond the next epoch.
	///
	/// Before importing the first block this returns `None`.
	pub fn slot_ticket(slot: Slot) -> Option<TicketBody> {
		if frame_system::Pallet::<T>::block_number().is_zero() {
			return None
		}

		let curr_epoch_idx = Self::current_epoch_index();
		let slot_epoch_idx = Self::epoch_index(slot);
		if slot_epoch_idx < curr_epoch_idx || slot_epoch_idx > curr_epoch_idx + 1 {
			return None
		}

		let mut epoch_tag = slot_epoch_idx.tag();
		let epoch_len = T::EpochLength::get();
		let mut slot_idx = Self::slot_index(slot);

		if epoch_len <= slot_idx && slot_idx < 2 * epoch_len {
			// Try to get a ticket for the next epoch. Since its state values were not enacted yet,
			// we may have to finish sorting the tickets.
			epoch_tag = slot_epoch_idx.next_tag();
			slot_idx -= epoch_len;
			if TicketsAccumulator::<T>::count() != 0 {
				Self::consume_tickets_accumulator(usize::MAX, epoch_tag);
			}
		} else if slot_idx >= 2 * epoch_len {
			return None
		}

		let mut tickets_count = TicketsCount::<T>::get();
		let tickets_count = tickets_count[epoch_tag as usize];
		if tickets_count <= slot_idx {
			return None
		}

		let get_ticket_index = |slot_index: u32| {
			let mut ticket_index = slot_idx / 2;
			if slot_index & 1 != 0 {
				ticket_index = tickets_count - (ticket_index + 1);
			}
			ticket_index as u32
		};

		let ticket_idx = get_ticket_index(slot_idx);
		debug!(
			target: LOG_TARGET,
			"slot-idx {} <-> ticket-idx {}",
			slot_idx,
			ticket_idx
		);
		Tickets::<T>::get((epoch_tag, ticket_idx))
	}

	/// Reset tickets related data.
	///
	/// Optimization note: tickets are left in place, only the associated counters are resetted.
	#[inline(always)]
	fn reset_tickets_data() {
		TicketsCount::<T>::kill();
		let _ = TicketsAccumulator::<T>::clear(u32::MAX, None);
	}

	/// Static protocol configuration.
	#[inline(always)]
	pub fn protocol_config() -> Configuration {
		Configuration {
			epoch_length: T::EpochLength::get(),
			epoch_tail_length: T::EpochLength::get() / EPOCH_TAIL_FRACTION,
			max_authorities: T::MaxAuthorities::get(),
			redundancy_factor: T::RedundancyFactor::get(),
			attempts_number: T::AttemptsNumber::get(),
		}
	}

	/// Current epoch information.
	#[inline(always)]
	pub fn current_epoch() -> Epoch {
		Epoch {
			start: Self::current_epoch_start(),
			authorities: Self::authorities().into_inner(),
			randomness: Self::randomness_buf(),
		}
	}

	/// Randomness buffer entries.
	///
	/// Assuming we're executing a block during epoch with index `N`.
	///
	/// Entries:
	/// - 0 : randomness accumulator after execution of previous block.
	/// - 1 : randomness accumulator snapshot after execution of epoch `N-1` last block.
	/// - 2 : randomness accumulator snapshot after execution of epoch `N-2` last block.
	/// - 3 : randomness accumulator snapshot after execution of epoch `N-3` last block.
	///
	/// The semantic of these entries is defined as:
	/// - 3 : epoch `N` randomness
	/// - 2 : epoch `N+1` randomness
	/// - 1 : epoch `N+2` randomness
	/// - 0 : accumulator for epoch `N+3` randomness
	///
	/// If `index` is greater than 3 the `Default` is returned.
	#[inline(always)]
	fn randomness(index: usize) -> Randomness {
		Self::randomness_buf().get(index).cloned().unwrap_or_default()
	}

	/// Current epoch's randomness.
	#[inline(always)]
	fn current_randomness() -> Randomness {
		Self::randomness(3)
	}

	/// Next epoch's randomness.
	#[inline(always)]
	fn next_randomness() -> Randomness {
		Self::randomness(2)
	}

	/// Randomness accumulator
	#[inline(always)]
	fn randomness_accumulator() -> Randomness {
		Self::randomness(0)
	}

	/// Determine whether an epoch change should take place at this block.
	#[inline(always)]
	fn should_end_epoch(block_num: BlockNumberFor<T>) -> bool {
		Self::current_slot_index() == 0 && block_num != Zero::zero()
	}

	/// Current slot index relative to the current epoch.
	#[inline(always)]
	fn current_slot_index() -> u32 {
		Self::slot_index(CurrentSlot::<T>::get())
	}

	/// Slot index relative to the current epoch.
	#[inline(always)]
	fn slot_index(slot: Slot) -> u32 {
		(*slot % <T as Config>::EpochLength::get() as u64) as u32
	}

	/// Current epoch index.
	#[inline(always)]
	fn current_epoch_index() -> u64 {
		Self::epoch_index(Self::current_slot())
	}

	/// Epoch's index from slot.
	#[inline(always)]
	fn epoch_index(slot: Slot) -> u64 {
		*slot / <T as Config>::EpochLength::get() as u64
	}

	/// Epoch length
	/// Get current epoch first slot.
	#[inline(always)]
	fn current_epoch_start() -> Slot {
		let curr_slot = *Self::current_slot();
		let epoch_start = curr_slot - curr_slot % <T as Config>::EpochLength::get() as u64;
		Slot::from(epoch_start)
	}

	/// Get the epoch's first slot.
	#[inline(always)]
	fn epoch_start(epoch_index: u64) -> Slot {
		const PROOF: &str = "slot number is u64; it should relate in some way to wall clock time; \
							 if u64 is not enough we should crash for safety; qed.";
		epoch_index.checked_mul(T::EpochLength::get() as u64).expect(PROOF).into()
	}

	#[inline(always)]
	fn epoch_length() -> u32 {
		T::EpochLength::get()
	}
}

/// Trigger an epoch change, if any should take place.
pub trait EpochChangeTrigger {
	/// May trigger an epoch change, if any should take place.
	///
	/// Returns an optional `Weight` if epoch change has been triggered.
	///
	/// This should be called during every block, after initialization is done.
	fn trigger<T: Config>(_: BlockNumberFor<T>) -> Weight;
}

/// An `EpochChangeTrigger` which does nothing.
///
/// In practice this means that the epoch change logic is left to some external component
/// (e.g. pallet-session).
pub struct EpochChangeExternalTrigger;

impl EpochChangeTrigger for EpochChangeExternalTrigger {
	fn trigger<T: Config>(_: BlockNumberFor<T>) -> Weight {
		// nothing - trigger is external.
		Weight::zero()
	}
}

/// An `EpochChangeTrigger` which recycle the same authorities set forever.
///
/// The internal trigger should only be used when no other module is responsible for
/// changing authority set.
pub struct EpochChangeInternalTrigger;

impl EpochChangeTrigger for EpochChangeInternalTrigger {
	fn trigger<T: Config>(block_num: BlockNumberFor<T>) -> Weight {
		if Pallet::<T>::should_end_epoch(block_num) {
			let authorities = Pallet::<T>::next_authorities();
			let next_authorities = authorities.clone();
			let len = next_authorities.len() as u32;
			Pallet::<T>::enact_epoch_change(authorities, next_authorities);
			T::WeightInfo::enact_epoch_change(len, T::EpochLength::get())
		} else {
			Weight::zero()
		}
	}
}

impl<T: Config> BoundToRuntimeAppPublic for Pallet<T> {
	type Public = AuthorityId;
}
