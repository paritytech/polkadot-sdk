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

//! Per-destination MMR accumulator for speculative cross-chain messaging.
//!
//! This pallet maintains:
//!
//! **Sender side:**
//! - A per-destination MMR for each parachain this chain sends messages to.
//!   Each message's hash is appended as a leaf. The MMR root for each
//!   destination is tracked in a top-level binary Merkle tree whose root
//!   becomes the [`ProvidesCommitment`].
//!
//! **Receiver side:**
//! - Per-source state tracking how many messages have been processed from
//!   each upstream source, and the last provides root built against.
//!   Processing a batch of messages produces a [`RequiresCommitment`].
//!
//! # Integration with `pallet-parachain-system`
//!
//! This pallet implements [`SpeculativeMessagingProvider`] so that
//! `pallet-parachain-system` can collect provides/requires commitments at
//! block finalization. Configure it via:
//!
//! ```ignore
//! type SpeculativeMessagingProvider = pallet_speculative_messaging::Pallet<Runtime>;
//! ```
//!
//! The public API methods are:
//!
//! - [`Pallet::send_message`] — append a message to a destination's MMR
//! - [`Pallet::receive_messages`] — process an incoming batch from a source
//! - [`Pallet::provides_commitment`] — get the current provides root
//! - [`Pallet::requires_commitments`] — get requires produced this block

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
pub mod inherent;
pub mod per_dest_mmr;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use per_dest_mmr::MmrState;

const LOG_TARGET: &str = "pallet-speculative-messaging";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::vec::Vec;
	use frame_support::pallet_prelude::*;
	use frame_system::{ensure_none, ensure_root, ensure_signed, pallet_prelude::*};
	use per_dest_mmr::MmrState;
	use polkadot_parachain_primitives::primitives::Id as ParaId;
	use polkadot_primitives_speculative_messaging::{
		OutgoingMessage, ProvidesCommitment, RequiresCommitment, SourceState, StoredMerkleTree,
	};
	use sp_core::H256;
	use sp_inherents::InherentIdentifier;

	/// Inherent identifier for speculative messaging.
	pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"specmsg0";

	/// The inherent data type: list of (source, count, provides_root) tuples.
	pub type InherentType = Vec<(ParaId, u64, H256)>;

	/// Current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Maximum number of destinations this chain can send messages to.
		#[pallet::constant]
		type MaxDestinations: Get<u32>;

		/// Maximum number of sources this chain can receive messages from
		/// in a single block.
		#[pallet::constant]
		type MaxSources: Get<u32>;

		/// Maximum number of messages that can be sent to a single
		/// destination in one block.
		#[pallet::constant]
		type MaxMessagesPerBlock: Get<u32>;

		/// Maximum payload size in bytes for a single message.
		#[pallet::constant]
		type MaxPayloadSize: Get<u32>;
	}

	// =========================================================================
	// Sender-side storage
	// =========================================================================

	/// Per-destination MMR state (leaf count + peaks).
	///
	/// Keyed by the destination `ParaId`. Each entry is the complete MMR
	/// state for messages sent to that destination across all blocks.
	#[pallet::storage]
	pub type DestinationMmrs<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, MmrState, OptionQuery>;

	/// The top-level Merkle tree over all per-destination MMR roots.
	///
	/// Updated incrementally as destination MMR roots change. Its root is
	/// the `ProvidesCommitment` published to the relay chain.
	#[pallet::storage]
	pub type TopLevelTree<T: Config> = StorageValue<_, StoredMerkleTree, ValueQuery>;

	/// Messages sent during the current block, keyed by destination.
	///
	/// Cleared at the start of each block via `on_initialize`. These are
	/// made available to collator networking so that peer chains can fetch
	/// the messages they need.
	#[pallet::storage]
	pub type PendingOutgoing<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, Vec<OutgoingMessage>, ValueQuery>;

	// =========================================================================
	// Receiver-side storage
	// =========================================================================

	/// Per-source state on the receiver side.
	///
	/// Tracks how many messages have been processed from each source and
	/// the last provides root we consumed against.
	#[pallet::storage]
	pub type PerSourceState<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, SourceState, ValueQuery>;

	/// `RequiresCommitment`s accumulated during the current block.
	///
	/// Each call to [`Pallet::receive_messages`] appends one entry. Cleared
	/// at the start of each block.
	#[pallet::storage]
	pub type PendingRequires<T: Config> = StorageValue<_, Vec<RequiresCommitment>, ValueQuery>;

	// =========================================================================
	// Peer registry storage
	// =========================================================================

	/// Maximum length of a serialized relay chain peer identity.
	///
	/// PeerIds are typically 38 bytes (multihash-encoded ed25519 public
	/// key). 64 bytes provides room for future peer ID formats.
	const MAX_PEER_ID_LEN: u32 = 64;

	/// Relay chain peer identities for speculative message routing.
	///
	/// Maps a destination `ParaId` to the opaque peer identity bytes of
	/// the relay chain node responsible for that parachain. This is the
	/// "hardcoded discovery" mechanism: collators read this map to know
	/// where to forward speculative messages.
	///
	/// Managed via the [`Pallet::set_relay_peer`] and
	/// [`Pallet::remove_relay_peer`] extrinsics (root-only for MVP).
	#[pallet::storage]
	pub type RelayPeers<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, BoundedVec<u8, ConstU32<MAX_PEER_ID_LEN>>, OptionQuery>;

	// =========================================================================
	// Events
	// =========================================================================

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A message was appended to a destination's MMR.
		MessageSent {
			destination: ParaId,
			position: u64,
			leaf_hash: H256,
		},
		/// A batch of messages from a source was processed.
		MessagesReceived {
			source: ParaId,
			count: u64,
			new_provides_root: H256,
		},
		/// A new destination was added to the top-level tree.
		DestinationAdded {
			destination: ParaId,
		},
		/// A relay chain peer was registered or updated for a parachain.
		RelayPeerUpdated {
			para_id: ParaId,
		},
		/// A relay chain peer registration was removed.
		RelayPeerRemoved {
			para_id: ParaId,
		},
	}

	// =========================================================================
	// Errors
	// =========================================================================

	#[pallet::error]
	pub enum Error<T> {
		/// Too many destinations; would exceed `MaxDestinations`.
		TooManyDestinations,
		/// Too many messages sent to a destination in this block.
		TooManyMessagesPerBlock,
		/// Message positions in the batch are not sequential from the
		/// expected start.
		InvalidMessageSequence,
		/// The batch is empty — nothing to process.
		EmptyBatch,
		/// Too many sources processed in this block.
		TooManySources,
		/// Payload exceeds `MaxPayloadSize`.
		PayloadTooLarge,
		/// This source has already been processed in this block.
		DuplicateSourceInBlock,
	}

	// =========================================================================
	// Hooks
	// =========================================================================

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Clear block-local storage from the previous block.
			let _ = PendingRequires::<T>::kill();
			// Use u32::MAX to guarantee full removal even if MaxDestinations
			// was lowered via a runtime upgrade since the entries were written.
			let removed = PendingOutgoing::<T>::clear(u32::MAX, None);

			// 1 write for PendingRequires::kill().
			// For PendingOutgoing::clear(): each entry requires a read + write.
			T::DbWeight::get().reads_writes(
				1 + removed.unique as u64,
				1 + removed.unique as u64,
			)
		}
	}

	// =========================================================================
	// Dispatchables (extrinsics)
	// =========================================================================

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register the relay chain peer identity for a given parachain.
		///
		/// This is the "hardcoded discovery" mechanism for the MVP.
		/// Collator networking reads this storage to know which relay
		/// chain peer handles speculative messages for each destination.
		///
		/// Only callable by root (sudo) for the MVP.
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_relay_peer(
			origin: OriginFor<T>,
			para_id: ParaId,
			peer_id: BoundedVec<u8, ConstU32<MAX_PEER_ID_LEN>>,
		) -> DispatchResult {
			ensure_root(origin)?;
			RelayPeers::<T>::insert(para_id, peer_id);
			Self::deposit_event(Event::RelayPeerUpdated { para_id });
			Ok(())
		}

		/// Remove the relay chain peer registration for a given
		/// parachain.
		///
		/// Only callable by root (sudo) for the MVP.
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn remove_relay_peer(
			origin: OriginFor<T>,
			para_id: ParaId,
		) -> DispatchResult {
			ensure_root(origin)?;
			RelayPeers::<T>::remove(para_id);
			Self::deposit_event(Event::RelayPeerRemoved { para_id });
			Ok(())
		}

		/// Process incoming speculative message metadata via inherent.
		///
		/// This is called automatically by the block builder when the
		/// inherent data provider has queued inbound message metadata.
		/// Only callable as an unsigned inherent.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::zero())]
		pub fn receive_messages_inherent(
			origin: OriginFor<T>,
			entries: Vec<(ParaId, u64, H256)>,
		) -> DispatchResult {
			ensure_none(origin)?;
			for (source, count, provides_root) in entries {
				Self::receive_messages(source, count, provides_root)?;
			}
			Ok(())
		}

		/// Send a speculative message to a destination parachain.
		///
		/// This is a signed extrinsic that wraps [`Pallet::send_message`]
		/// for use in testing and MVP scenarios.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::zero())]
		pub fn send_message_extrinsic(
			origin: OriginFor<T>,
			destination: ParaId,
			payload: Vec<u8>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			Self::send_message(destination, payload)?;
			Ok(())
		}
	}

	// =========================================================================
	// Pallet implementation (public API — not dispatchables)
	// =========================================================================

	impl<T: Config> Pallet<T> {
		/// Append a message to the per-destination MMR for `destination`.
		///
		/// This is the sender-side entry point. It:
		/// 1. Loads (or creates) the MMR for `destination`.
		/// 2. Computes the leaf hash from the message.
		/// 3. Pushes the leaf into the MMR.
		/// 4. Updates the top-level Merkle tree with the new MMR root.
		/// 5. Stores the message in `PendingOutgoing` for collator access.
		pub fn send_message(
			destination: ParaId,
			payload: Vec<u8>,
		) -> Result<(u64, H256), DispatchError> {
			ensure!(
				payload.len() as u32 <= T::MaxPayloadSize::get(),
				Error::<T>::PayloadTooLarge
			);

			// Check per-block message limit before any mutations.
			let pending_count = PendingOutgoing::<T>::decode_len(destination).unwrap_or(0);
			ensure!(
				(pending_count as u32) < T::MaxMessagesPerBlock::get(),
				Error::<T>::TooManyMessagesPerBlock
			);

			let mut mmr = DestinationMmrs::<T>::get(destination).unwrap_or_default();
			let position = mmr.leaf_count;

			let message = OutgoingMessage { destination, payload, position };
			let leaf_hash = message.leaf_hash();

			let new_mmr_root = mmr.push(leaf_hash);

			// Update the top-level Merkle tree.
			TopLevelTree::<T>::mutate(|tree| {
				let is_new = tree.get_destination_root(destination).is_none();
				if is_new {
					// Check destination limit before adding.
					ensure!(
						(tree.len() as u32) < T::MaxDestinations::get(),
						Error::<T>::TooManyDestinations
					);
					tree.upsert(destination, new_mmr_root);
					Self::deposit_event(Event::DestinationAdded { destination });
				} else {
					tree.update(destination, new_mmr_root).map_err(|_| {
						// This should never happen since we just checked it exists.
						Error::<T>::TooManyDestinations
					})?;
				}
				Ok::<(), DispatchError>(())
			})?;

			// Persist MMR state.
			DestinationMmrs::<T>::insert(destination, mmr);

			// Append to pending outgoing for this block.
			PendingOutgoing::<T>::mutate(destination, |msgs| {
				msgs.push(message);
			});

			Self::deposit_event(Event::MessageSent { destination, position, leaf_hash });

			log::debug!(
				target: LOG_TARGET,
				"Sent message to {:?}, position={}, leaf_hash={:?}",
				destination, position, leaf_hash,
			);

			Ok((position, leaf_hash))
		}

		/// Process a batch of incoming messages from `source`.
		///
		/// This is the receiver-side entry point. It:
		/// 1. Validates that the batch is non-empty and positions are
		///    sequential from where we left off.
		/// 2. Advances the per-source state.
		/// 3. Records a `RequiresCommitment` for this source.
		///
		/// **Note:** Message content verification (MMR leaf proofs) is
		/// expected to be done by the caller or by the PVF. This method
		/// only tracks the bookkeeping state.
		pub fn receive_messages(
			source: ParaId,
			count: u64,
			provides_root: H256,
		) -> Result<(), DispatchError> {
			ensure!(count > 0, Error::<T>::EmptyBatch);

			// Check limits before any mutations.
			let requires = PendingRequires::<T>::get();
			ensure!(
				(requires.len() as u32) < T::MaxSources::get(),
				Error::<T>::TooManySources
			);
			ensure!(
				!requires.iter().any(|r| r.source == source),
				Error::<T>::DuplicateSourceInBlock
			);
			drop(requires);

			PerSourceState::<T>::mutate(source, |state| {
				state
					.advance(count, provides_root)
					.map_err(|_| Error::<T>::InvalidMessageSequence)
			})?;

			PendingRequires::<T>::mutate(|requires| {
				requires.push(RequiresCommitment { source, expected_root: provides_root });
			});

			Self::deposit_event(Event::MessagesReceived {
				source,
				count,
				new_provides_root: provides_root,
			});

			log::debug!(
				target: LOG_TARGET,
				"Received {} messages from {:?}, provides_root={:?}",
				count, source, provides_root,
			);

			Ok(())
		}

		/// Returns the current `ProvidesCommitment` (top-level Merkle root
		/// over all per-destination MMR roots).
		pub fn provides_commitment() -> ProvidesCommitment {
			TopLevelTree::<T>::get().provides_commitment()
		}

		/// Returns the `RequiresCommitment`s accumulated during this block.
		pub fn requires_commitments() -> Vec<RequiresCommitment> {
			PendingRequires::<T>::get()
		}

		/// Returns the MMR state for a given destination, if it exists.
		pub fn destination_mmr(destination: ParaId) -> Option<MmrState> {
			DestinationMmrs::<T>::get(destination)
		}

		/// Returns the receiver-side state for a given source.
		pub fn source_state(source: ParaId) -> SourceState {
			PerSourceState::<T>::get(source)
		}

		/// Returns the outgoing messages for a destination in the current
		/// block.
		pub fn pending_outgoing(destination: ParaId) -> Vec<OutgoingMessage> {
			PendingOutgoing::<T>::get(destination)
		}

		/// Returns the top-level Merkle tree (for proof generation).
		pub fn top_level_tree() -> StoredMerkleTree {
			TopLevelTree::<T>::get()
		}

		/// Returns the relay chain peer identity registered for the
		/// given parachain, if any.
		pub fn relay_peer(para_id: ParaId) -> Option<Vec<u8>> {
			RelayPeers::<T>::get(para_id).map(|bounded| bounded.into_inner())
		}

		/// Returns all registered relay chain peer entries.
		pub fn all_relay_peers() -> Vec<(ParaId, Vec<u8>)> {
			RelayPeers::<T>::iter()
				.map(|(para_id, peer_id)| (para_id, peer_id.into_inner()))
				.collect()
		}
	}

	// =========================================================================
	// Inherent support
	// =========================================================================

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = sp_inherents::MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &sp_inherents::InherentData) -> Option<Self::Call> {
			let entries: InherentType = data.get_data(&INHERENT_IDENTIFIER).ok()??;
			if entries.is_empty() {
				return None;
			}
			Some(Call::receive_messages_inherent { entries })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::receive_messages_inherent { .. })
		}
	}
}

// =========================================================================
// SpeculativeMessagingProvider implementation
// =========================================================================

use polkadot_primitives_speculative_messaging::SpeculativeMessagingProvider;

impl<T: Config> SpeculativeMessagingProvider for Pallet<T> {
	fn provides_root() -> Option<sp_core::H256> {
		let commitment = Self::provides_commitment();
		if commitment.is_empty() {
			None
		} else {
			Some(commitment.root)
		}
	}

	fn requires_commitments()
		-> alloc::vec::Vec<polkadot_primitives_speculative_messaging::RequiresCommitment>
	{
		Self::requires_commitments()
	}
}
