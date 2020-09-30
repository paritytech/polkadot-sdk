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

use crate::storage::{AuthoritySet, ImportedHeader, ScheduledChange};
use codec::{Codec, EncodeLike};
use frame_support::{
	decl_error, decl_module, decl_storage,
	dispatch::{DispatchResult, Parameter},
};
use frame_system::ensure_signed;
use num_traits::AsPrimitive;
use sp_runtime::traits::{
	AtLeast32BitUnsigned, Hash as HashT, Header as HeaderT, MaybeDisplay, MaybeMallocSizeOf, MaybeSerializeDeserialize,
	Member, SimpleBitOps,
};
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*, str::FromStr};

mod justification;
mod storage;
mod storage_proof;
mod verifier;

#[cfg(test)]
mod mock;

pub trait Trait: frame_system::Trait {
	/// A type that fulfills the abstract idea of what a Substrate header is.
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html
	type BridgedHeader: Parameter + HeaderT<Number = Self::BridgedBlockNumber, Hash = Self::BridgedBlockHash>;

	/// A type that fulfills the abstract idea of what a Substrate block number is.
	// Constraits come from the associated Number type of `sp_runtime::traits::Header`
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html#associatedtype.Number
	//
	// Note that the `AsPrimitive<usize>` trait is required by the Grandpa justification
	// verifier, and is not usually part of a Substrate Header's Number type.
	type BridgedBlockNumber: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Debug
		+ sp_std::hash::Hash
		+ Copy
		+ MaybeDisplay
		+ AtLeast32BitUnsigned
		+ Codec
		+ FromStr
		+ MaybeMallocSizeOf
		+ AsPrimitive<usize>;

	/// A type that fulfills the abstract idea of what a Substrate hash is.
	// Constraits come from the associated Hash type of `sp_runtime::traits::Header`
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html#associatedtype.Hash
	type BridgedBlockHash: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Debug
		+ sp_std::hash::Hash
		+ Ord
		+ Copy
		+ MaybeDisplay
		+ Default
		+ SimpleBitOps
		+ Codec
		+ AsRef<[u8]>
		+ AsMut<[u8]>
		+ MaybeMallocSizeOf
		+ EncodeLike;

	/// A type that fulfills the abstract idea of what a Substrate hasher (a type
	/// that produces hashes) is.
	// Constraits come from the associated Hashing type of `sp_runtime::traits::Header`
	// See here for more info:
	// https://crates.parity.io/sp_runtime/traits/trait.Header.html#associatedtype.Hashing
	type BridgedBlockHasher: HashT<Output = Self::BridgedBlockHash>;
}

decl_storage! {
	trait Store for Module<T: Trait> as SubstrateBridge {
		/// Hash of the best finalized header.
		BestFinalized: T::BridgedBlockHash;
		/// Headers which have been imported into the pallet.
		ImportedHeaders: map hasher(identity) T::BridgedBlockHash => Option<ImportedHeader<T::BridgedHeader>>;
		/// The current Grandpa Authority set.
		CurrentAuthoritySet: AuthoritySet;
		/// The next scheduled authority set change.
		// Grandpa doesn't require there to always be a pending change. In fact, most of the time
		// there will be no pending change available.
		NextScheduledChange: Option<ScheduledChange<T::BridgedBlockNumber>>;
	}
	add_extra_genesis {
		config(initial_header): Option<T::BridgedHeader>;
		config(initial_authority_list): sp_finality_grandpa::AuthorityList;
		config(initial_set_id): sp_finality_grandpa::SetId;
		config(first_scheduled_change): Option<ScheduledChange<T::BridgedBlockNumber>>;
		build(|config| {
			assert!(
				!config.initial_authority_list.is_empty(),
				"An initial authority list is needed."
			);

			let initial_header = config
				.initial_header
				.clone()
				.expect("An initial header is needed");

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
			header: T::BridgedHeader,
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
			hash: T::BridgedBlockHash,
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

/// Expected interface for interacting with bridge pallet storage.
// TODO: This should be split into its own less-Substrate-dependent crate
pub trait BridgeStorage {
	/// The header type being used by the pallet.
	type Header: HeaderT;

	/// Write a header to storage.
	fn write_header(&mut self, header: &ImportedHeader<Self::Header>);

	/// Get the best finalized header the pallet knows of.
	fn best_finalized_header(&self) -> ImportedHeader<Self::Header>;

	/// Update the best finalized header the pallet knows of.
	fn update_best_finalized(&self, hash: <Self::Header as HeaderT>::Hash);

	/// Check if a particular header is known to the pallet.
	fn header_exists(&self, hash: <Self::Header as HeaderT>::Hash) -> bool;

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
	type Header = T::BridgedHeader;

	fn write_header(&mut self, header: &ImportedHeader<T::BridgedHeader>) {
		let hash = header.header.hash();
		<ImportedHeaders<T>>::insert(hash, header);
	}

	fn best_finalized_header(&self) -> ImportedHeader<T::BridgedHeader> {
		let hash = <BestFinalized<T>>::get();
		self.header_by_hash(hash)
			.expect("A finalized header was added at genesis, therefore this must always exist")
	}

	fn update_best_finalized(&self, hash: T::BridgedBlockHash) {
		<BestFinalized<T>>::put(hash)
	}

	fn header_exists(&self, hash: T::BridgedBlockHash) -> bool {
		<ImportedHeaders<T>>::contains_key(hash)
	}

	fn header_by_hash(&self, hash: T::BridgedBlockHash) -> Option<ImportedHeader<T::BridgedHeader>> {
		<ImportedHeaders<T>>::get(hash)
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

	fn scheduled_set_change(&self) -> Option<ScheduledChange<T::BridgedBlockNumber>> {
		<NextScheduledChange<T>>::get()
	}

	fn schedule_next_set_change(&self, next_change: ScheduledChange<T::BridgedBlockNumber>) {
		<NextScheduledChange<T>>::put(next_change)
	}
}
