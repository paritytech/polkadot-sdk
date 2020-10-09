// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate Bridge Pallet
//!
//! This pallet is an on-chain light client for chains which have a notion of finality.
//!
//! It has a simple interface for achieving this. First it can import headers to the runtime
//! storage. During this it will check the validity of the headers and ensure they don't conflict
//! with any existing headers (e.g they're on a different finalized chain). Secondly it can finalize
//! an already imported header (and its ancestors) given a valid Grandpa justification.
//!
//! With these two functions the pallet is able to form a "source of truth" for what headers have
//! been finalized on a given Substrate chain. This can be a useful source of info for other
//! higher-level applications.

#![cfg_attr(not(feature = "std"), no_std)]
// Runtime-generated enums
#![allow(clippy::large_enum_variant)]

use crate::storage::ImportedHeader;
use bp_runtime::{BlockNumberOf, Chain, HashOf, HeaderOf};
use frame_support::{decl_error, decl_module, decl_storage, dispatch::DispatchResult};
use frame_system::ensure_signed;
use sp_runtime::traits::Header as HeaderT;
use sp_std::{marker::PhantomData, prelude::*};

// Re-export since the node uses these when configuring genesis
pub use storage::{AuthoritySet, ScheduledChange};

mod justification;
mod storage;
mod storage_proof;
mod verifier;

#[cfg(test)]
mod mock;

pub trait Trait: frame_system::Trait {
	/// Chain that we are bridging here.
	type BridgedChain: Chain;
}

/// Block number of the bridged chain.
pub(crate) type BridgedBlockNumber<T> = BlockNumberOf<<T as Trait>::BridgedChain>;
/// Block hash of the bridged chain.
pub(crate) type BridgedBlockHash<T> = HashOf<<T as Trait>::BridgedChain>;
/// Header of the bridged chain.
pub(crate) type BridgedHeader<T> = HeaderOf<<T as Trait>::BridgedChain>;

decl_storage! {
	trait Store for Module<T: Trait> as SubstrateBridge {
		/// Hash of the header at the highest known height.
		BestHeader: BridgedBlockHash<T>;
		/// Hash of the best finalized header.
		BestFinalized: BridgedBlockHash<T>;
		/// A header which enacts an authority set change and therefore
		/// requires a Grandpa justification.
		// Since we won't always have an authority set change scheduled we
		// won't always have a header which needs a justification.
		RequiresJustification: Option<BridgedBlockHash<T>>;
		/// Headers which have been imported into the pallet.
		ImportedHeaders: map hasher(identity) BridgedBlockHash<T> => Option<ImportedHeader<BridgedHeader<T>>>;
		/// The current Grandpa Authority set.
		CurrentAuthoritySet: AuthoritySet;
		/// The next scheduled authority set change.
		// Grandpa doesn't require there to always be a pending change. In fact, most of the time
		// there will be no pending change available.
		NextScheduledChange: Option<ScheduledChange<BridgedBlockNumber<T>>>;
	}
	add_extra_genesis {
		config(initial_header): Option<BridgedHeader<T>>;
		config(initial_authority_list): sp_finality_grandpa::AuthorityList;
		config(initial_set_id): sp_finality_grandpa::SetId;
		config(first_scheduled_change): Option<ScheduledChange<BridgedBlockNumber<T>>>;
		build(|config| {
			assert!(
				!config.initial_authority_list.is_empty(),
				"An initial authority list is needed."
			);

			let initial_header = config
				.initial_header
				.clone()
				.expect("An initial header is needed");

			<BestHeader<T>>::put(initial_header.hash());
			<BestFinalized<T>>::put(initial_header.hash());
			<ImportedHeaders<T>>::insert(
				initial_header.hash(),
				ImportedHeader {
					header: initial_header,
					requires_justification: false,
					is_finalized: true,
				},
			);

			let authority_set =
				AuthoritySet::new(config.initial_authority_list.clone(), config.initial_set_id);
			CurrentAuthoritySet::put(authority_set);

			if let Some(ref change) = config.first_scheduled_change {
				<NextScheduledChange<T>>::put(change);
			};
		})
	}
}

decl_error! {
	pub enum Error for Module<T: Trait> {
		/// This header has failed basic verification.
		InvalidHeader,
		/// This header has not been finalized.
		UnfinalizedHeader,
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		type Error = Error<T>;

		/// Import a signed Substrate header into the runtime.
		///
		/// This will perform some basic checks to make sure it is fine to
		/// import into the runtime. However, it does not perform any checks
		/// related to finality.
		// TODO: Update weights [#78]
		#[weight = 0]
		pub fn import_signed_header(
			origin,
			header: BridgedHeader<T>,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			frame_support::debug::trace!(target: "sub-bridge", "Got header {:?}", header);

			let mut verifier = verifier::Verifier {
				storage: PalletStorage::<T>::new(),
			};

			let _ = verifier
				.import_header(header)
				.map_err(|_| <Error<T>>::InvalidHeader)?;

			Ok(())
		}

		/// Import a finalty proof for a particular header.
		///
		/// This will take care of finalizing any already imported headers
		/// which get finalized when importing this particular proof, as well
		/// as updating the current and next validator sets.
		// TODO: Update weights [#78]
		#[weight = 0]
		pub fn finalize_header(
			origin,
			hash: BridgedBlockHash<T>,
			finality_proof: Vec<u8>,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			frame_support::debug::trace!(target: "sub-bridge", "Got header hash {:?}", hash);

			let mut verifier = verifier::Verifier {
				storage: PalletStorage::<T>::new(),
			};

			let _ = verifier
				.import_finality_proof(hash, finality_proof.into())
				.map_err(|_| <Error<T>>::UnfinalizedHeader)?;

			Ok(())
		}
	}
}

impl<T: Trait> Module<T> {
	/// Get the highest header that the pallet knows of.
	// In a future where we support forks this could be a Vec of headers
	// since we may have multiple headers at the same height.
	pub fn best_header() -> BridgedHeader<T> {
		PalletStorage::<T>::new().best_header().header
	}

	/// Get the best finalized header the pallet knows of.
	///
	/// Since this has been finalized correctly a user of the bridge
	/// pallet should be confident that any transactions that were
	/// included in this or any previous header will not be reverted.
	pub fn best_finalized() -> BridgedHeader<T> {
		PalletStorage::<T>::new().best_finalized_header().header
	}

	/// Check if a particular header is known to the bridge pallet.
	pub fn is_known_header(hash: BridgedBlockHash<T>) -> bool {
		PalletStorage::<T>::new().header_exists(hash)
	}

	/// Check if a particular header is finalized.
	///
	/// Will return false if the header is not known to the pallet.
	// One thing worth noting here is that this approach won't work well
	// once we track forks since there could be an older header on a
	// different fork which isn't an ancestor of our best finalized header.
	pub fn is_finalized_header(hash: BridgedBlockHash<T>) -> bool {
		let storage = PalletStorage::<T>::new();
		if let Some(header) = storage.header_by_hash(hash) {
			header.number() <= storage.best_finalized_header().number()
		} else {
			false
		}
	}

	/// Return the latest header which enacts an authority set change
	/// and still needs a finality proof.
	///
	/// Will return None if there are no headers which are missing finality proofs.
	pub fn requires_justification() -> Option<BridgedHeader<T>> {
		let storage = PalletStorage::<T>::new();
		let hash = storage.unfinalized_header()?;
		let imported_header = storage.header_by_hash(hash).expect(
			"We write a header to storage before marking it as unfinalized, therefore
			this must always exist if we got an unfinalized header hash.",
		);
		Some(imported_header.header)
	}
}

/// Expected interface for interacting with bridge pallet storage.
// TODO: This should be split into its own less-Substrate-dependent crate
pub trait BridgeStorage {
	/// The header type being used by the pallet.
	type Header: HeaderT;

	/// Write a header to storage.
	fn write_header(&mut self, header: &ImportedHeader<Self::Header>);

	/// Get the header at the highest known height.
	fn best_header(&self) -> ImportedHeader<Self::Header>;

	/// Update the header at the highest height.
	fn update_best_header(&mut self, hash: <Self::Header as HeaderT>::Hash);

	/// Get the best finalized header the pallet knows of.
	fn best_finalized_header(&self) -> ImportedHeader<Self::Header>;

	/// Update the best finalized header the pallet knows of.
	fn update_best_finalized(&self, hash: <Self::Header as HeaderT>::Hash);

	/// Check if a particular header is known to the pallet.
	fn header_exists(&self, hash: <Self::Header as HeaderT>::Hash) -> bool;

	/// Return a header which requires a justification. A header will require
	/// a justification when it enacts an new authority set.
	fn unfinalized_header(&self) -> Option<<Self::Header as HeaderT>::Hash>;

	/// Mark a header as eventually requiring a justification.
	///
	/// If None is passed the storage item is cleared.
	fn update_unfinalized_header(&mut self, hash: Option<<Self::Header as HeaderT>::Hash>);

	/// Get a specific header by its hash.
	///
	/// Returns None if it is not known to the pallet.
	fn header_by_hash(&self, hash: <Self::Header as HeaderT>::Hash) -> Option<ImportedHeader<Self::Header>>;

	/// Get the current Grandpa authority set.
	fn current_authority_set(&self) -> AuthoritySet;

	/// Update the current Grandpa authority set.
	///
	/// Should only be updated when a scheduled change has been triggered.
	fn update_current_authority_set(&self, new_set: AuthoritySet);

	/// Replace the current authority set with the next scheduled set.
	///
	/// Returns an error if there is no scheduled authority set to enact.
	fn enact_authority_set(&mut self) -> Result<(), ()>;

	/// Get the next scheduled Grandpa authority set change.
	fn scheduled_set_change(&self) -> Option<ScheduledChange<<Self::Header as HeaderT>::Number>>;

	/// Schedule a Grandpa authority set change in the future.
	fn schedule_next_set_change(&self, next_change: ScheduledChange<<Self::Header as HeaderT>::Number>);
}

/// Used to interact with the pallet storage in a more abstract way.
#[derive(Default, Clone)]
pub struct PalletStorage<T>(PhantomData<T>);

impl<T> PalletStorage<T> {
	fn new() -> Self {
		Self(PhantomData::<T>::default())
	}
}

impl<T: Trait> BridgeStorage for PalletStorage<T> {
	type Header = BridgedHeader<T>;

	fn write_header(&mut self, header: &ImportedHeader<BridgedHeader<T>>) {
		let hash = header.header.hash();
		<ImportedHeaders<T>>::insert(hash, header);
	}

	fn best_header(&self) -> ImportedHeader<BridgedHeader<T>> {
		let hash = <BestHeader<T>>::get();
		self.header_by_hash(hash)
			.expect("A header must have been written at genesis, therefore this must always exist")
	}

	fn update_best_header(&mut self, hash: BridgedBlockHash<T>) {
		<BestHeader<T>>::put(hash)
	}

	fn best_finalized_header(&self) -> ImportedHeader<BridgedHeader<T>> {
		let hash = <BestFinalized<T>>::get();
		self.header_by_hash(hash)
			.expect("A finalized header was added at genesis, therefore this must always exist")
	}

	fn update_best_finalized(&self, hash: BridgedBlockHash<T>) {
		<BestFinalized<T>>::put(hash)
	}

	fn header_exists(&self, hash: BridgedBlockHash<T>) -> bool {
		<ImportedHeaders<T>>::contains_key(hash)
	}

	fn header_by_hash(&self, hash: BridgedBlockHash<T>) -> Option<ImportedHeader<BridgedHeader<T>>> {
		<ImportedHeaders<T>>::get(hash)
	}

	fn unfinalized_header(&self) -> Option<BridgedBlockHash<T>> {
		<RequiresJustification<T>>::get()
	}

	fn update_unfinalized_header(&mut self, hash: Option<<Self::Header as HeaderT>::Hash>) {
		if let Some(hash) = hash {
			<RequiresJustification<T>>::put(hash);
		} else {
			<RequiresJustification<T>>::kill();
		}
	}

	fn current_authority_set(&self) -> AuthoritySet {
		CurrentAuthoritySet::get()
	}

	fn update_current_authority_set(&self, new_set: AuthoritySet) {
		CurrentAuthoritySet::put(new_set)
	}

	fn enact_authority_set(&mut self) -> Result<(), ()> {
		if <NextScheduledChange<T>>::exists() {
			let new_set = <NextScheduledChange<T>>::take()
				.expect("Ensured that entry existed in storage")
				.authority_set;
			self.update_current_authority_set(new_set);

			Ok(())
		} else {
			Err(())
		}
	}

	fn scheduled_set_change(&self) -> Option<ScheduledChange<BridgedBlockNumber<T>>> {
		<NextScheduledChange<T>>::get()
	}

	fn schedule_next_set_change(&self, next_change: ScheduledChange<BridgedBlockNumber<T>>) {
		<NextScheduledChange<T>>::put(next_change)
	}
}
