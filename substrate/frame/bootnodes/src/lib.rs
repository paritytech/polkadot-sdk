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
//! genesis hash, this para's id, the address and an expiry slot. The expiry slot is a consent
//! lease: readers drop an entry whose slot has passed on their view of JAM, so an operator that
//! stops re-signing is de-listed everywhere and an old signature is only usable until its slot.
//! The runtime stores the slot and does not compare it against JAM time.
//!
//! The address must be `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>`, optionally followed by
//! `/ws` or `/wss`: the shapes a node can dial a bootnode on. The runtime does not otherwise
//! interpret it.
//!
//! The set can be seeded at genesis with entries signed ahead of time: `bootnodes-cli sign
//! --offline` prints, as `bootnode`, an entry in the JSON form the genesis config takes.
//!
//! Publishing the set to the para-owned key-value store is not implemented yet. Readers consume
//! the pallet's storage directly.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `add_bootnode`: add a bootnode, or replace the entry of an already listed node key. Requires
//!   `AdminOrigin`.
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
use frame_support::{
	pallet_prelude::*, CloneNoBound, DebugNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{ed25519, H256};

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::BenchmarkHelper;
pub use pallet::*;
pub use weights::WeightInfo;

/// Domain separator prefixed to the bytes a node signs to prove possession of its key.
pub const POP_DOMAIN: &[u8] = b"bootnode-pop-v1";

/// A bootnode entry, as stored on chain and as submitted to [`Pallet::add_bootnode`].
///
/// The `serde` form, which the genesis config and so chain specs use, writes every byte field as
/// a `0x`-prefixed hex string.
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
	Serialize,
	Deserialize,
)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(MaxAddrLen))]
#[serde(bound(serialize = "", deserialize = ""))]
pub struct Bootnode<MaxAddrLen: Get<u32>> {
	/// The node's network address as a binary multiaddr.
	///
	/// Must be `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>[/ws|/wss]`, without a `/p2p/`
	/// component: the peer id is derived from `node_key`.
	#[serde(with = "hex")]
	pub addr: BoundedVec<u8, MaxAddrLen>,
	/// The ed25519 public key the node's peer id is derived from.
	#[serde(with = "hex")]
	pub node_key: ed25519::Public,
	/// Consent lease: the JAM timeslot after which readers drop the entry.
	pub expiry_slot: u32,
	/// Proof of possession: signature by `node_key` over [`pop_payload`].
	#[serde(with = "hex")]
	pub sig: ed25519::Signature,
}

/// The [`Bootnode`] type as configured for `T`.
pub type BootnodeOf<T> = Bootnode<<T as Config>::MaxAddrLen>;

/// `serde` representation of the byte fields of [`Bootnode`]: `0x`-prefixed hex, like other byte
/// blobs in chain specs.
///
/// `sp_core`'s own impls are not used because they write a public key as an SS58 address, and a
/// node key is not an account.
mod hex {
	use super::*;

	/// A byte field of [`Bootnode`].
	pub trait Bytes: AsRef<[u8]> + Sized {
		/// The length the field accepts, for error messages.
		const EXPECTED: &'static str;

		/// Build the field from raw bytes, or `None` if their length is not accepted.
		fn from_bytes(bytes: Vec<u8>) -> Option<Self>;
	}

	impl<L: Get<u32>> Bytes for BoundedVec<u8, L> {
		const EXPECTED: &'static str = "at most `MaxAddrLen` bytes";

		fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
			Self::try_from(bytes).ok()
		}
	}

	impl Bytes for ed25519::Public {
		const EXPECTED: &'static str = "32 bytes";

		fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
			Self::try_from(bytes.as_slice()).ok()
		}
	}

	impl Bytes for ed25519::Signature {
		const EXPECTED: &'static str = "64 bytes";

		fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
			Self::try_from(bytes.as_slice()).ok()
		}
	}

	pub fn serialize<S: serde::Serializer, T: Bytes>(
		bytes: &T,
		serializer: S,
	) -> Result<S::Ok, S::Error> {
		sp_core::bytes::serialize(bytes.as_ref(), serializer)
	}

	pub fn deserialize<'de, D: serde::Deserializer<'de>, T: Bytes>(
		deserializer: D,
	) -> Result<T, D::Error> {
		let bytes: Vec<u8> = sp_core::bytes::deserialize(deserializer)?;
		let len = bytes.len();
		T::from_bytes(bytes).ok_or_else(|| {
			serde::de::Error::custom(alloc::format!("expected {}, got {len} bytes", T::EXPECTED))
		})
	}
}

/// The bytes a node signs to prove it consents to being listed as a bootnode of `para_id`.
///
/// Layout: [`POP_DOMAIN`] followed by `SCALE((jam_genesis_hash, para_id, addr, expiry_slot))`.
/// The domain is prepended raw, without a length prefix; `addr` is encoded as a length-prefixed
/// byte sequence, like a `Vec<u8>`.
pub fn pop_payload(
	jam_genesis_hash: &H256,
	para_id: u32,
	addr: &[u8],
	expiry_slot: u32,
) -> Vec<u8> {
	let mut payload = POP_DOMAIN.to_vec();
	(jam_genesis_hash, para_id, addr, expiry_slot).encode_to(&mut payload);
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

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Bootnodes listed from genesis. Each must carry a valid proof of possession.
		pub bootnodes: Vec<BootnodeOf<T>>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for bootnode in &self.bootnodes {
				Pallet::<T>::do_add_bootnode(bootnode.clone())
					.expect("invalid bootnode in genesis config");
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A bootnode was added to the set.
		BootnodeAdded {
			node_key: ed25519::Public,
			addr: BoundedVec<u8, T::MaxAddrLen>,
			expiry_slot: u32,
		},
		/// A listed bootnode's entry was replaced.
		BootnodeUpdated {
			node_key: ed25519::Public,
			addr: BoundedVec<u8, T::MaxAddrLen>,
			expiry_slot: u32,
		},
		/// A bootnode was removed from the set.
		BootnodeRemoved { node_key: ed25519::Public },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The address is not `/(ip4|ip6|dns|dns4|dns6)/<host>/tcp/<port>[/ws|/wss]`.
		InvalidMultiaddr,
		/// The address contains a `/p2p/` component; the peer id is derived from the node key.
		MultiaddrContainsPeerId,
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
		/// `bootnode.addr` must pass [`multiaddr::validate`] and `bootnode.sig` must be the node's
		/// signature over [`pop_payload`].
		///
		/// Emits `BootnodeAdded` for a new node key and `BootnodeUpdated` for a listed one.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::add_bootnode())]
		pub fn add_bootnode(origin: OriginFor<T>, bootnode: BootnodeOf<T>) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Self::do_add_bootnode(bootnode)
		}

		/// Remove the bootnode with the given node key from the set.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove_bootnode())]
		pub fn remove_bootnode(origin: OriginFor<T>, node_key: ed25519::Public) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Self::do_remove_bootnode(&node_key)
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The bytes a node must sign to be listed as a bootnode of this para at `addr` until
	/// `expiry_slot`.
	pub fn pop_payload(addr: &[u8], expiry_slot: u32) -> Vec<u8> {
		pop_payload(&T::JamGenesisHash::get(), T::ParaId::get(), addr, expiry_slot)
	}

	/// Whether `bootnode.sig` is a valid proof of possession for the entry.
	pub fn verify_pop(bootnode: &BootnodeOf<T>) -> bool {
		let payload = Self::pop_payload(&bootnode.addr, bootnode.expiry_slot);
		sp_io::crypto::ed25519_verify(&bootnode.sig, &payload, &bootnode.node_key)
	}

	/// Validate `bootnode` and insert it, replacing an existing entry for the same node key.
	pub fn do_add_bootnode(bootnode: BootnodeOf<T>) -> DispatchResult {
		multiaddr::validate(&bootnode.addr).map_err(|e| match e {
			multiaddr::Error::ContainsPeerId => Error::<T>::MultiaddrContainsPeerId,
			_ => Error::<T>::InvalidMultiaddr,
		})?;
		let mut bootnodes = Bootnodes::<T>::get();
		let position = bootnodes.iter().position(|b| b.node_key == bootnode.node_key);
		ensure!(position.is_some() || !bootnodes.is_full(), Error::<T>::TooManyBootnodes);
		ensure!(Self::verify_pop(&bootnode), Error::<T>::InvalidSignature);

		let node_key = bootnode.node_key;
		let addr = bootnode.addr.clone();
		let expiry_slot = bootnode.expiry_slot;
		match position {
			Some(index) => bootnodes[index] = bootnode,
			None => bootnodes.force_push(bootnode),
		}
		Bootnodes::<T>::put(bootnodes);

		Self::deposit_event(match position {
			Some(_) => Event::BootnodeUpdated { node_key, addr, expiry_slot },
			None => Event::BootnodeAdded { node_key, addr, expiry_slot },
		});
		Ok(())
	}

	/// Remove the entry for `node_key` from the set.
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
		}
		Ok(())
	}
}
