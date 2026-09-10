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

//! # Bootnodes Pallet
//!
//! Maintains the set of bootnodes a parachain advertises, so that nodes joining the network can
//! discover the chain's peers from on-chain state instead of an operator-provided list.
//!
//! ## Overview
//!
//! The pallet stores up to `MaxBootnodes` entries. Each entry names a node by the ed25519
//! network key its libp2p peer id is derived from, together with the node's network address as a
//! binary multiaddr without a `/p2p/` component.
//!
//! Admission into the set is an administrative decision (`AdminOrigin`, typically governance),
//! but no node can be listed without its operator's consent: every entry carries a proof of
//! possession, a signature by the node key over a domain-separated payload that binds the JAM
//! genesis hash, this para's id, a sequence number and the address. The sequence number must
//! strictly exceed the last one accepted for that node key and stay below `u64::MAX`, so a later
//! entry is always possible. The last accepted value is kept after removal, so an old signature
//! can never be replayed to re-list a node.
//!
//! The address must be `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>`, optionally followed by
//! `/ws` or `/wss`: the shapes a node can dial a bootnode on. The runtime does not otherwise
//! interpret it.
//!
//! Publishing the set to the para-owned key-value store is not implemented yet. Readers consume
//! the pallet's storage directly.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `add_bootnode`: add a bootnode, or replace the entry of an already listed node key with one
//!   carrying a higher sequence number. Requires `AdminOrigin`.
//! - `remove_bootnode`: remove a bootnode by node key. Requires `AdminOrigin`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
pub mod multiaddr;
#[cfg(test)]
mod tests;
pub mod weights;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use sp_core::{ed25519, H256};

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::BenchmarkHelper;
pub use pallet::*;
pub use weights::WeightInfo;

/// Domain separator prefixed to the bytes a node signs to prove possession of its key.
pub const POP_DOMAIN: &[u8] = b"bootnode-pop-v1";

/// A bootnode entry, as stored on chain and as submitted to [`Pallet::add_bootnode`].
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	DebugNoBound,
)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(MaxAddrLen))]
pub struct Bootnode<MaxAddrLen: Get<u32>> {
	/// The node's network address as a binary multiaddr.
	///
	/// Must be `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>[/ws|/wss]`, without a `/p2p/`
	/// component: the peer id is derived from `node_key`.
	pub addr: BoundedVec<u8, MaxAddrLen>,
	/// The ed25519 public key the node's peer id is derived from.
	pub node_key: ed25519::Public,
	/// Sequence number, strictly increasing per `node_key` and below `u64::MAX`, preventing
	/// replay of old entries.
	pub seq: u64,
	/// Proof of possession: signature by `node_key` over [`pop_payload`].
	pub sig: ed25519::Signature,
}

/// The [`Bootnode`] type as configured for `T`.
pub type BootnodeOf<T> = Bootnode<<T as Config>::MaxAddrLen>;

/// The bytes a node signs to prove it consents to being listed as a bootnode of `para_id`.
///
/// Layout: [`POP_DOMAIN`] followed by `SCALE((jam_genesis_hash, para_id, seq, addr))`. The domain
/// is prepended raw, without a length prefix; `addr` is encoded as a length-prefixed byte
/// sequence, like a `Vec<u8>`.
pub fn pop_payload(jam_genesis_hash: &H256, para_id: u32, seq: u64, addr: &[u8]) -> Vec<u8> {
	let mut payload = POP_DOMAIN.to_vec();
	(jam_genesis_hash, para_id, seq, addr).encode_to(&mut payload);
	payload
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Origin allowed to add and remove bootnodes. Typically governance.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// This parachain's id. Part of the signed proof-of-possession payload.
		#[pallet::constant]
		type ParaId: Get<u32>;

		/// Genesis hash of the JAM chain this parachain runs on. Part of the signed payload.
		#[pallet::constant]
		type JamGenesisHash: Get<H256>;

		/// Maximum number of bootnodes in the set. The SpecMsg address record allows 4.
		///
		/// Must not be lowered below the stored data without a migration.
		#[pallet::constant]
		type MaxBootnodes: Get<u32>;

		/// Maximum length in bytes of a bootnode's binary multiaddr.
		///
		/// Must not be lowered below the stored data without a migration.
		#[pallet::constant]
		type MaxAddrLen: Get<u32>;

		/// Weight information for the extrinsics of this pallet.
		type WeightInfo: WeightInfo;

		/// Produces node keys and proof-of-possession signatures for benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper;
	}

	/// The active bootnode set, in insertion order. Node keys are unique within the set.
	#[pallet::storage]
	pub type Bootnodes<T: Config> =
		StorageValue<_, BoundedVec<BootnodeOf<T>, T::MaxBootnodes>, ValueQuery>;

	/// The highest sequence number accepted for a node key.
	///
	/// Kept after the node is removed from the set, so that a removed node cannot be re-listed
	/// with an old signature. Only `AdminOrigin` can grow this map.
	#[pallet::storage]
	pub type LastSeq<T: Config> =
		StorageMap<_, Blake2_128Concat, ed25519::Public, u64, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A bootnode was added to the set.
		BootnodeAdded { node_key: ed25519::Public, seq: u64 },
		/// A listed bootnode's entry was replaced by one with a higher sequence number.
		BootnodeUpdated { node_key: ed25519::Public, seq: u64 },
		/// A bootnode was removed from the set.
		BootnodeRemoved { node_key: ed25519::Public },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The address is not `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>[/ws|/wss]`.
		InvalidMultiaddr,
		/// The address contains a `/p2p/` component; the peer id is derived from the node key.
		MultiaddrContainsPeerId,
		/// The sequence number does not exceed the last one accepted for this node key.
		StaleSeq,
		/// The sequence number is `u64::MAX`, which no later entry could exceed.
		SeqTooLarge,
		/// The proof of possession does not verify against the node key.
		InvalidSignature,
		/// The set already holds `MaxBootnodes` entries.
		TooManyBootnodes,
		/// No bootnode with this node key is listed.
		UnknownBootnode,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Add a bootnode, or replace the entry of an already listed node key.
		///
		/// `bootnode.addr` must pass [`multiaddr::validate`], `bootnode.seq`
		/// must exceed the last sequence number accepted for `bootnode.node_key`, and
		/// `bootnode.sig` must be the node's signature over [`pop_payload`].
		///
		/// Emits `BootnodeAdded` for a new node key and `BootnodeUpdated` for a listed one.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::add_bootnode())]
		pub fn add_bootnode(origin: OriginFor<T>, bootnode: BootnodeOf<T>) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Self::do_add_bootnode(bootnode)
		}

		/// Remove the bootnode with the given node key from the set.
		///
		/// The last accepted sequence number for the key is retained.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove_bootnode())]
		pub fn remove_bootnode(origin: OriginFor<T>, node_key: ed25519::Public) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Self::do_remove_bootnode(&node_key)
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The bytes a node must sign to be listed as a bootnode of this para with `seq` and `addr`.
	pub fn pop_payload(seq: u64, addr: &[u8]) -> Vec<u8> {
		pop_payload(&T::JamGenesisHash::get(), T::ParaId::get(), seq, addr)
	}

	/// Whether `bootnode.sig` is a valid proof of possession for the entry.
	pub fn verify_pop(bootnode: &BootnodeOf<T>) -> bool {
		let payload = Self::pop_payload(bootnode.seq, &bootnode.addr);
		sp_io::crypto::ed25519_verify(&bootnode.sig, &payload, &bootnode.node_key)
	}

	/// Validate `bootnode` and insert it, replacing an existing entry for the same node key.
	pub fn do_add_bootnode(bootnode: BootnodeOf<T>) -> DispatchResult {
		multiaddr::validate(&bootnode.addr).map_err(|e| match e {
			multiaddr::Error::ContainsPeerId => Error::<T>::MultiaddrContainsPeerId,
			_ => Error::<T>::InvalidMultiaddr,
		})?;
		ensure!(bootnode.seq < u64::MAX, Error::<T>::SeqTooLarge);
		if let Some(last) = LastSeq::<T>::get(bootnode.node_key) {
			ensure!(bootnode.seq > last, Error::<T>::StaleSeq);
		}
		let mut bootnodes = Bootnodes::<T>::get();
		let position = bootnodes.iter().position(|b| b.node_key == bootnode.node_key);
		ensure!(position.is_some() || !bootnodes.is_full(), Error::<T>::TooManyBootnodes);
		ensure!(Self::verify_pop(&bootnode), Error::<T>::InvalidSignature);

		let node_key = bootnode.node_key;
		let seq = bootnode.seq;
		match position {
			Some(index) => bootnodes[index] = bootnode,
			None => bootnodes.force_push(bootnode),
		}
		Bootnodes::<T>::put(bootnodes);
		LastSeq::<T>::insert(node_key, seq);

		Self::deposit_event(match position {
			Some(_) => Event::BootnodeUpdated { node_key, seq },
			None => Event::BootnodeAdded { node_key, seq },
		});
		Ok(())
	}

	/// Remove the entry for `node_key` from the set, keeping its last accepted sequence number.
	pub fn do_remove_bootnode(node_key: &ed25519::Public) -> DispatchResult {
		Bootnodes::<T>::try_mutate(|bootnodes| -> DispatchResult {
			let pos = bootnodes
				.iter()
				.position(|b| &b.node_key == node_key)
				.ok_or(Error::<T>::UnknownBootnode)?;
			bootnodes.remove(pos);
			Ok(())
		})?;
		Self::deposit_event(Event::BootnodeRemoved { node_key: *node_key });
		Ok(())
	}

	/// Check the pallet's storage invariants.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		let mut seen = alloc::collections::BTreeSet::new();
		for bootnode in Bootnodes::<T>::get().iter() {
			ensure!(seen.insert(bootnode.node_key), "duplicate node key in the bootnode set");
			ensure!(
				multiaddr::validate(&bootnode.addr).is_ok(),
				"invalid multiaddr in the bootnode set"
			);
			ensure!(
				LastSeq::<T>::get(bootnode.node_key) == Some(bootnode.seq),
				"listed bootnode's seq is not the last accepted seq for its key"
			);
		}
		Ok(())
	}
}
