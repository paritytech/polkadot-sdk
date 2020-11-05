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
use bp_runtime::{BlockNumberOf, Chain, HashOf, HasherOf, HeaderOf};
use frame_support::{decl_error, decl_module, decl_storage, dispatch::DispatchResult};
use frame_system::ensure_signed;
use sp_runtime::traits::Header as HeaderT;
use sp_runtime::RuntimeDebug;
use sp_std::{marker::PhantomData, prelude::*};
use sp_trie::StorageProof;

// Re-export since the node uses these when configuring genesis
pub use storage::{AuthoritySet, ScheduledChange};

pub use justification::decode_justification_target;
pub use storage_proof::StorageProofChecker;

mod justification;
mod storage;
mod storage_proof;
mod verifier;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod fork_tests;

/// Block number of the bridged chain.
pub(crate) type BridgedBlockNumber<T> = BlockNumberOf<<T as Trait>::BridgedChain>;
/// Block hash of the bridged chain.
pub(crate) type BridgedBlockHash<T> = HashOf<<T as Trait>::BridgedChain>;
/// Hasher of the bridged chain.
pub(crate) type BridgedBlockHasher<T> = HasherOf<<T as Trait>::BridgedChain>;
/// Header of the bridged chain.
pub(crate) type BridgedHeader<T> = HeaderOf<<T as Trait>::BridgedChain>;

/// A convenience type identifying headers.
#[derive(RuntimeDebug, PartialEq)]
pub struct HeaderId<H: HeaderT> {
	/// The block number of the header.
	pub number: H::Number,
	/// The hash of the header.
	pub hash: H::Hash,
}

pub trait Trait: frame_system::Trait {
	/// Chain that we are bridging here.
	type BridgedChain: Chain;
}

decl_storage! {
	trait Store for Module<T: Trait> as SubstrateBridge {
		/// The number of the highest block(s) we know of.
		BestHeight: BridgedBlockNumber<T>;
		/// Hash of the header at the highest known height.
		///
		/// If there are multiple headers at the same "best" height
		/// this will contain all of their hashes.
		BestHeaders: Vec<BridgedBlockHash<T>>;
		/// Hash of the best finalized header.
		BestFinalized: BridgedBlockHash<T>;
		/// The set of header IDs (number, hash) which enact an authority set change and therefore
		/// require a Grandpa justification.
		RequiresJustification: map hasher(identity) BridgedBlockHash<T> => BridgedBlockNumber<T>;
		/// Headers which have been imported into the pallet.
		ImportedHeaders: map hasher(identity) BridgedBlockHash<T> => Option<ImportedHeader<BridgedHeader<T>>>;
		/// The current Grandpa Authority set.
		CurrentAuthoritySet: AuthoritySet;
		/// The next scheduled authority set change for a given fork.
		///
		/// The fork is indicated by the header which _signals_ the change (key in the mapping).
		/// Note that this is different than a header which _enacts_ a change.
		// Grandpa doesn't require there to always be a pending change. In fact, most of the time
		// there will be no pending change available.
		NextScheduledChange: map hasher(identity) BridgedBlockHash<T> => Option<ScheduledChange<BridgedBlockNumber<T>>>;
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
			let initial_hash = initial_header.hash();

			<BestHeight<T>>::put(initial_header.number());
			<BestHeaders<T>>::put(vec![initial_hash]);
			<BestFinalized<T>>::put(initial_hash);

			let authority_set =
				AuthoritySet::new(config.initial_authority_list.clone(), config.initial_set_id);
			CurrentAuthoritySet::put(authority_set);

			let mut signal_hash = None;
			if let Some(ref change) = config.first_scheduled_change {
				assert!(
					change.height > *initial_header.number(),
					"Changes must be scheduled past initial header."
				);

				signal_hash = Some(initial_hash);
				<NextScheduledChange<T>>::insert(initial_hash, change);
			};

			<ImportedHeaders<T>>::insert(
				initial_hash,
				ImportedHeader {
					header: initial_header,
					requires_justification: false,
					is_finalized: true,
					signal_hash,
				},
			);

		})
	}
}

decl_error! {
	pub enum Error for Module<T: Trait> {
		/// This header has failed basic verification.
		InvalidHeader,
		/// This header has not been finalized.
		UnfinalizedHeader,
		/// The header is unknown.
		UnknownHeader,
		/// The storage proof doesn't contains storage root. So it is invalid for given header.
		StorageRootMismatch,
		/// Error when trying to fetch storage value from the proof.
		StorageValueUnavailable,
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
	/// Get the highest header(s) that the pallet knows of.
	pub fn best_headers() -> Vec<(BridgedBlockNumber<T>, BridgedBlockHash<T>)> {
		PalletStorage::<T>::new()
			.best_headers()
			.iter()
			.map(|id| (id.number, id.hash))
			.collect()
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
			header.is_finalized
		} else {
			false
		}
	}

	/// Returns a list of headers which require finality proofs.
	///
	/// These headers require proofs because they enact authority set changes.
	pub fn require_justifications() -> Vec<(BridgedBlockNumber<T>, BridgedBlockHash<T>)> {
		PalletStorage::<T>::new()
			.missing_justifications()
			.iter()
			.map(|id| (id.number, id.hash))
			.collect()
	}

	/// Verify that the passed storage proof is valid, given it is crafted using
	/// known finalized header. If the proof is valid, then the `parse` callback
	/// is called and the function returns its result.
	pub fn parse_finalized_storage_proof<R>(
		finalized_header_hash: BridgedBlockHash<T>,
		storage_proof: StorageProof,
		parse: impl FnOnce(StorageProofChecker<BridgedBlockHasher<T>>) -> R,
	) -> Result<R, sp_runtime::DispatchError> {
		let storage = PalletStorage::<T>::new();
		let header = storage
			.header_by_hash(finalized_header_hash)
			.ok_or(Error::<T>::UnknownHeader)?;
		if !header.is_finalized {
			return Err(Error::<T>::UnfinalizedHeader.into());
		}

		let storage_proof_checker =
			StorageProofChecker::new(*header.state_root(), storage_proof).map_err(Error::<T>::from)?;
		Ok(parse(storage_proof_checker))
	}
}

/// Expected interface for interacting with bridge pallet storage.
// TODO: This should be split into its own less-Substrate-dependent crate
pub trait BridgeStorage {
	/// The header type being used by the pallet.
	type Header: HeaderT;

	/// Write a header to storage.
	fn write_header(&mut self, header: &ImportedHeader<Self::Header>);

	/// Get the header(s) at the highest known height.
	fn best_headers(&self) -> Vec<HeaderId<Self::Header>>;

	/// Get the best finalized header the pallet knows of.
	fn best_finalized_header(&self) -> ImportedHeader<Self::Header>;

	/// Update the best finalized header the pallet knows of.
	fn update_best_finalized(&self, hash: <Self::Header as HeaderT>::Hash);

	/// Check if a particular header is known to the pallet.
	fn header_exists(&self, hash: <Self::Header as HeaderT>::Hash) -> bool;

	/// Returns a list of headers which require justifications.
	///
	/// A header will require a justification if it enacts a new authority set.
	fn missing_justifications(&self) -> Vec<HeaderId<Self::Header>>;

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
	fn enact_authority_set(&mut self, signal_hash: <Self::Header as HeaderT>::Hash) -> Result<(), ()>;

	/// Get the next scheduled Grandpa authority set change.
	fn scheduled_set_change(
		&self,
		signal_hash: <Self::Header as HeaderT>::Hash,
	) -> Option<ScheduledChange<<Self::Header as HeaderT>::Number>>;

	/// Schedule a Grandpa authority set change in the future.
	///
	/// Takes the hash of the header which scheduled this particular change.
	fn schedule_next_set_change(
		&mut self,
		signal_hash: <Self::Header as HeaderT>::Hash,
		next_change: ScheduledChange<<Self::Header as HeaderT>::Number>,
	);
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
		use core::cmp::Ordering;

		let hash = header.hash();
		let current_height = header.number();
		let best_height = <BestHeight<T>>::get();

		match current_height.cmp(&best_height) {
			Ordering::Equal => {
				<BestHeaders<T>>::append(hash);
			}
			Ordering::Greater => {
				<BestHeaders<T>>::kill();
				<BestHeaders<T>>::append(hash);
				<BestHeight<T>>::put(current_height);
			}
			Ordering::Less => {
				// This is fine. We can still have a valid header, but it might just be on a
				// different fork and at a lower height than the "best" overall header.
			}
		}

		if header.requires_justification {
			<RequiresJustification<T>>::insert(hash, current_height);
		} else {
			// If the key doesn't exist this is a no-op, so it's fine to call it often
			<RequiresJustification<T>>::remove(hash);
		}

		<ImportedHeaders<T>>::insert(hash, header);
	}

	fn best_headers(&self) -> Vec<HeaderId<BridgedHeader<T>>> {
		let number = <BestHeight<T>>::get();
		<BestHeaders<T>>::get()
			.iter()
			.map(|hash| HeaderId { number, hash: *hash })
			.collect()
	}

	fn best_finalized_header(&self) -> ImportedHeader<BridgedHeader<T>> {
		let hash = <BestFinalized<T>>::get();
		self.header_by_hash(hash)
			.expect("A finalized header was added at genesis, therefore this must always exist")
	}

	fn update_best_finalized(&self, hash: BridgedBlockHash<T>) {
		<BestFinalized<T>>::put(hash);
	}

	fn header_exists(&self, hash: BridgedBlockHash<T>) -> bool {
		<ImportedHeaders<T>>::contains_key(hash)
	}

	fn header_by_hash(&self, hash: BridgedBlockHash<T>) -> Option<ImportedHeader<BridgedHeader<T>>> {
		<ImportedHeaders<T>>::get(hash)
	}

	fn missing_justifications(&self) -> Vec<HeaderId<BridgedHeader<T>>> {
		<RequiresJustification<T>>::iter()
			.map(|(hash, number)| HeaderId { number, hash })
			.collect()
	}

	fn current_authority_set(&self) -> AuthoritySet {
		CurrentAuthoritySet::get()
	}

	fn update_current_authority_set(&self, new_set: AuthoritySet) {
		CurrentAuthoritySet::put(new_set)
	}

	fn enact_authority_set(&mut self, signal_hash: BridgedBlockHash<T>) -> Result<(), ()> {
		let new_set = <NextScheduledChange<T>>::take(signal_hash).ok_or(())?.authority_set;
		self.update_current_authority_set(new_set);

		Ok(())
	}

	fn scheduled_set_change(&self, signal_hash: BridgedBlockHash<T>) -> Option<ScheduledChange<BridgedBlockNumber<T>>> {
		<NextScheduledChange<T>>::get(signal_hash)
	}

	fn schedule_next_set_change(
		&mut self,
		signal_hash: BridgedBlockHash<T>,
		next_change: ScheduledChange<BridgedBlockNumber<T>>,
	) {
		<NextScheduledChange<T>>::insert(signal_hash, next_change)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{helpers::unfinalized_header, run_test, TestRuntime};
	use frame_support::{assert_noop, assert_ok};

	#[test]
	fn parse_finalized_storage_proof_rejects_proof_on_unknown_header() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime>::parse_finalized_storage_proof(
					Default::default(),
					StorageProof::new(vec![]),
					|_| (),
				),
				Error::<TestRuntime>::UnknownHeader,
			);
		});
	}

	#[test]
	fn parse_finalized_storage_proof_rejects_proof_on_unfinalized_header() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let header = unfinalized_header(1);
			storage.write_header(&header);

			assert_noop!(
				Module::<TestRuntime>::parse_finalized_storage_proof(
					header.header.hash(),
					StorageProof::new(vec![]),
					|_| (),
				),
				Error::<TestRuntime>::UnfinalizedHeader,
			);
		});
	}

	#[test]
	fn parse_finalized_storage_accepts_valid_proof() {
		run_test(|| {
			let mut storage = PalletStorage::<TestRuntime>::new();
			let (state_root, storage_proof) = storage_proof::tests::craft_valid_storage_proof();
			let mut header = unfinalized_header(1);
			header.is_finalized = true;
			header.header.set_state_root(state_root);
			storage.write_header(&header);

			assert_ok!(
				Module::<TestRuntime>::parse_finalized_storage_proof(header.header.hash(), storage_proof, |_| (),),
				(),
			);
		});
	}
}
