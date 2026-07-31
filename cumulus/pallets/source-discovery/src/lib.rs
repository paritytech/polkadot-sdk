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

//! # Cross-parachain source discovery configuration
//!
//! A receiver parachain records, per source [`ParaId`], how to reach that
//! source's collators over the relay-chain DHT — its 32-byte genesis hash and an
//! optional fork id — via the governance-gated [`Pallet::set_source_genesis`].
//! The node's discovery client reads it (through
//! `cumulus_primitives_source_discovery::SourceDiscoveryApi::source_discovery_info`,
//! backed by [`Pallet::source_discovery_info`]) and resolves that source's peers.
//!
//! This is *reachability config only* — the pallet is independent of any
//! messaging mechanics, and a runtime that doesn't include it (or configures no
//! sources) runs no cross-parachain discovery.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use alloc::vec::Vec;
	use cumulus_primitives_core::ParaId;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// An (optional) parachain fork id — same 32-byte bound as other polkadot
	/// protocol identifiers.
	pub type ForkId = BoundedVec<u8, ConstU32<32>>;
	/// Stored reachability for a source: its 32-byte genesis hash + optional fork id.
	pub type SourceInfoOf = ([u8; 32], Option<ForkId>);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Origin allowed to set a source's discovery info (governance).
		type SetSourceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// This parachain's own id — a chain cannot configure itself as a source.
		type SelfParaId: Get<ParaId>;
		/// Maximum number of configured sources. Bounds the map the runtime API
		/// materializes and the node walks each block; a *new* source beyond this
		/// is rejected (updates to an existing source are always allowed).
		#[pallet::constant]
		type MaxSources: Get<u32>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Per-source reachability: `source -> (genesis, fork_id)`. Governance-set;
	/// the node reads it to discover that source's peers. One entry per source
	/// para covers every channel to it — genesis is per-chain.
	#[pallet::storage]
	pub type SourceGenesis<T: Config> =
		CountedStorageMap<_, Twox64Concat, ParaId, SourceInfoOf, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A source's discovery info was set/updated, or cleared (`info: None`).
		SourceGenesisSet {
			/// The source parachain.
			source: ParaId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A chain cannot configure itself as a source.
		SelfSource,
		/// The configured-source limit ([`Config::MaxSources`]) is reached.
		TooManySources,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set — or clear, with `info: None` — how to reach `source`'s collators
		/// over the relay-chain DHT. Governance-gated ([`Config::SetSourceOrigin`]).
		/// This is the receiver's own record of a source's reachability, which
		/// the channel handshake can't supply (fetching a source's messages
		/// requires already knowing how to reach it).
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
		pub fn set_source_genesis(
			origin: OriginFor<T>,
			source: ParaId,
			info: Option<SourceInfoOf>,
		) -> DispatchResult {
			T::SetSourceOrigin::ensure_origin(origin)?;
			ensure!(source != T::SelfParaId::get(), Error::<T>::SelfSource);
			match info {
				Some(info) => {
					// Only a *new* source counts against the cap; updates are always allowed.
					if !SourceGenesis::<T>::contains_key(source) {
						ensure!(
							SourceGenesis::<T>::count() < T::MaxSources::get(),
							Error::<T>::TooManySources,
						);
					}
					SourceGenesis::<T>::insert(source, info);
				},
				None => SourceGenesis::<T>::remove(source),
			}
			Self::deposit_event(Event::SourceGenesisSet { source });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The configured sources and how to reach each — the data behind
		/// `SourceDiscoveryApi::source_discovery_info`. Empty ⇒ no cross-parachain
		/// discovery is configured.
		pub fn source_discovery_info() -> Vec<(ParaId, ([u8; 32], Option<Vec<u8>>))> {
			SourceGenesis::<T>::iter()
				.map(|(source, (genesis, fork))| {
					(source, (genesis, fork.map(BoundedVec::into_inner)))
				})
				.collect()
		}
	}
}
