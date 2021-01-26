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
//! an already imported header (and its ancestors) given a valid GRANDPA justification.
//!
//! With these two functions the pallet is able to form a "source of truth" for what headers have
//! been finalized on a given Substrate chain. This can be a useful source of info for other
//! higher-level applications.

#![cfg_attr(not(feature = "std"), no_std)]
// Runtime-generated enums
#![allow(clippy::large_enum_variant)]

use crate::storage::ImportedHeader;
use bp_header_chain::AuthoritySet;
use bp_runtime::{BlockNumberOf, Chain, HashOf, HasherOf, HeaderOf};
use frame_support::{
	decl_error, decl_module, decl_storage, dispatch::DispatchResult, ensure, traits::Get, weights::DispatchClass,
};
use frame_system::{ensure_signed, RawOrigin};
use sp_runtime::traits::Header as HeaderT;
use sp_runtime::{traits::BadOrigin, RuntimeDebug};
use sp_std::{marker::PhantomData, prelude::*};
use sp_trie::StorageProof;

// Re-export since the node uses these when configuring genesis
pub use storage::{InitializationData, ScheduledChange};

pub use storage_proof::StorageProofChecker;

mod storage;
mod storage_proof;
mod verifier;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod fork_tests;

/// Block number of the bridged chain.
pub(crate) type BridgedBlockNumber<T> = BlockNumberOf<<T as Config>::BridgedChain>;
/// Block hash of the bridged chain.
pub(crate) type BridgedBlockHash<T> = HashOf<<T as Config>::BridgedChain>;
/// Hasher of the bridged chain.
pub(crate) type BridgedBlockHasher<T> = HasherOf<<T as Config>::BridgedChain>;
/// Header of the bridged chain.
pub(crate) type BridgedHeader<T> = HeaderOf<<T as Config>::BridgedChain>;

/// A convenience type identifying headers.
#[derive(RuntimeDebug, PartialEq)]
pub struct HeaderId<H: HeaderT> {
	/// The block number of the header.
	pub number: H::Number,
	/// The hash of the header.
	pub hash: H::Hash,
}

pub trait Config: frame_system::Config {
	/// Chain that we are bridging here.
	type BridgedChain: Chain;
}

decl_storage! {
	trait Store for Module<T: Config> as SubstrateBridge {
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
		/// require a GRANDPA justification.
		RequiresJustification: map hasher(identity) BridgedBlockHash<T> => BridgedBlockNumber<T>;
		/// Headers which have been imported into the pallet.
		ImportedHeaders: map hasher(identity) BridgedBlockHash<T> => Option<ImportedHeader<BridgedHeader<T>>>;
		/// The current GRANDPA Authority set.
		CurrentAuthoritySet: AuthoritySet;
		/// The next scheduled authority set change for a given fork.
		///
		/// The fork is indicated by the header which _signals_ the change (key in the mapping).
		/// Note that this is different than a header which _enacts_ a change.
		// GRANDPA doesn't require there to always be a pending change. In fact, most of the time
		// there will be no pending change available.
		NextScheduledChange: map hasher(identity) BridgedBlockHash<T> => Option<ScheduledChange<BridgedBlockNumber<T>>>;
		/// Optional pallet owner.
		///
		/// Pallet owner has a right to halt all pallet operations and then resume it. If it is
		/// `None`, then there are no direct ways to halt/resume pallet operations, but other
		/// runtime methods may still be used to do that (i.e. democracy::referendum to update halt
		/// flag directly or call the `halt_operations`).
		ModuleOwner get(fn module_owner): Option<T::AccountId>;
		/// If true, all pallet transactions are failed immediately.
		IsHalted get(fn is_halted): bool;
	}
	add_extra_genesis {
		config(owner): Option<T::AccountId>;
		config(init_data): Option<InitializationData<BridgedHeader<T>>>;
		build(|config| {
			if let Some(ref owner) = config.owner {
				<ModuleOwner<T>>::put(owner);
			}

			if let Some(init_data) = config.init_data.clone() {
				initialize_bridge::<T>(init_data);
			} else {
				// Since the bridge hasn't been initialized we shouldn't allow anyone to perform
				// transactions.
				IsHalted::put(true);
			}
		})
	}
}

decl_error! {
	pub enum Error for Module<T: Config> {
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
		/// All pallet operations are halted.
		Halted,
		/// The pallet has already been initialized.
		AlreadyInitialized,
	}
}

decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
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
			ensure_operational::<T>()?;
			let _ = ensure_signed(origin)?;
			let hash = header.hash();
			frame_support::debug::trace!("Going to import header {:?}: {:?}", hash, header);

			let mut verifier = verifier::Verifier {
				storage: PalletStorage::<T>::new(),
			};

			let _ = verifier
				.import_header(hash, header)
				.map_err(|e| {
					frame_support::debug::error!("Failed to import header {:?}: {:?}", hash, e);
					<Error<T>>::InvalidHeader
				})?;

			frame_support::debug::trace!("Successfully imported header: {:?}", hash);

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
			ensure_operational::<T>()?;
			let _ = ensure_signed(origin)?;
			frame_support::debug::trace!("Going to finalize header: {:?}", hash);

			let mut verifier = verifier::Verifier {
				storage: PalletStorage::<T>::new(),
			};

			let _ = verifier
				.import_finality_proof(hash, finality_proof.into())
				.map_err(|e| {
					frame_support::debug::error!("Failed to finalize header {:?}: {:?}", hash, e);
					<Error<T>>::UnfinalizedHeader
				})?;

			frame_support::debug::trace!("Successfully finalized header: {:?}", hash);

			Ok(())
		}

		/// Bootstrap the bridge pallet with an initial header and authority set from which to sync.
		///
		/// The initial configuration provided does not need to be the genesis header of the bridged
		/// chain, it can be any arbirary header. You can also provide the next scheduled set change
		/// if it is already know.
		///
		/// This function is only allowed to be called from a trusted origin and writes to storage
		/// with practically no checks in terms of the validity of the data. It is important that
		/// you ensure that valid data is being passed in.
		//TODO: Update weights [#78]
		#[weight = 0]
		pub fn initialize(
			origin,
			init_data: InitializationData<BridgedHeader<T>>,
		) {
			ensure_owner_or_root::<T>(origin)?;
			let init_allowed = !<BestFinalized<T>>::exists();
			ensure!(init_allowed, <Error<T>>::AlreadyInitialized);
			initialize_bridge::<T>(init_data.clone());

			frame_support::debug::info!(
				"Pallet has been initialized with the following parameters: {:?}", init_data
			);
		}

		/// Change `ModuleOwner`.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[weight = (T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational)]
		pub fn set_owner(origin, new_owner: Option<T::AccountId>) {
			ensure_owner_or_root::<T>(origin)?;
			match new_owner {
				Some(new_owner) => {
					ModuleOwner::<T>::put(&new_owner);
					frame_support::debug::info!("Setting pallet Owner to: {:?}", new_owner);
				},
				None => {
					ModuleOwner::<T>::kill();
					frame_support::debug::info!("Removed Owner of pallet.");
				},
			}
		}

		/// Halt all pallet operations. Operations may be resumed using `resume_operations` call.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[weight = (T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational)]
		pub fn halt_operations(origin) {
			ensure_owner_or_root::<T>(origin)?;
			IsHalted::put(true);
			frame_support::debug::warn!("Stopping pallet operations.");
		}

		/// Resume all pallet operations. May be called even if pallet is halted.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[weight = (T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational)]
		pub fn resume_operations(origin) {
			ensure_owner_or_root::<T>(origin)?;
			IsHalted::put(false);
			frame_support::debug::info!("Resuming pallet operations.");
		}
	}
}

impl<T: Config> Module<T> {
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
	/// Returns a dummy header if there is no best header. This can only happen
	/// if the pallet has not been initialized yet.
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

impl<T: Config> bp_header_chain::HeaderChain<BridgedHeader<T>> for Module<T> {
	fn best_finalized() -> BridgedHeader<T> {
		PalletStorage::<T>::new().best_finalized_header().header
	}

	fn authority_set() -> AuthoritySet {
		PalletStorage::<T>::new().current_authority_set()
	}

	fn import_header(header: BridgedHeader<T>) -> Result<(), ()> {
		let mut verifier = verifier::Verifier {
			storage: PalletStorage::<T>::new(),
		};

		let _ = verifier.import_header(header.hash(), header).map_err(|_| ())?;

		Ok(())
	}

	fn import_finality_proof(header: BridgedHeader<T>, finality_proof: Vec<u8>) -> Result<(), ()> {
		let mut verifier = verifier::Verifier {
			storage: PalletStorage::<T>::new(),
		};

		let _ = verifier
			.import_finality_proof(header.hash(), finality_proof.into())
			.map_err(|_| ())?;

		Ok(())
	}
}

/// Ensure that the origin is either root, or `ModuleOwner`.
fn ensure_owner_or_root<T: Config>(origin: T::Origin) -> Result<(), BadOrigin> {
	match origin.into() {
		Ok(RawOrigin::Root) => Ok(()),
		Ok(RawOrigin::Signed(ref signer)) if Some(signer) == <Module<T>>::module_owner().as_ref() => Ok(()),
		_ => Err(BadOrigin),
	}
}

/// Ensure that the pallet is in operational mode (not halted).
fn ensure_operational<T: Config>() -> Result<(), Error<T>> {
	if IsHalted::get() {
		Err(<Error<T>>::Halted)
	} else {
		Ok(())
	}
}

/// (Re)initialize bridge with given header for using it in external benchmarks.
#[cfg(feature = "runtime-benchmarks")]
pub fn initialize_for_benchmarks<T: Config>(header: HeaderOf<T::BridgedChain>) {
	initialize_bridge::<T>(InitializationData {
		header,
		authority_list: Vec::new(), // we don't verify any proofs in external benchmarks
		set_id: 0,
		scheduled_change: None,
		is_halted: false,
	});
}

/// Since this writes to storage with no real checks this should only be used in functions that were
/// called by a trusted origin.
fn initialize_bridge<T: Config>(init_params: InitializationData<BridgedHeader<T>>) {
	let InitializationData {
		header,
		authority_list,
		set_id,
		scheduled_change,
		is_halted,
	} = init_params;

	let initial_hash = header.hash();

	let mut signal_hash = None;
	if let Some(ref change) = scheduled_change {
		assert!(
			change.height > *header.number(),
			"Changes must be scheduled past initial header."
		);

		signal_hash = Some(initial_hash);
		<NextScheduledChange<T>>::insert(initial_hash, change);
	};

	<BestHeight<T>>::put(header.number());
	<BestHeaders<T>>::put(vec![initial_hash]);
	<BestFinalized<T>>::put(initial_hash);

	let authority_set = AuthoritySet::new(authority_list, set_id);
	CurrentAuthoritySet::put(authority_set);

	<ImportedHeaders<T>>::insert(
		initial_hash,
		ImportedHeader {
			header,
			requires_justification: false,
			is_finalized: true,
			signal_hash,
		},
	);

	IsHalted::put(is_halted);
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
	///
	/// Returns None if there is no best header. This can only happen if the pallet
	/// has not been initialized yet.
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

	/// Get the current GRANDPA authority set.
	fn current_authority_set(&self) -> AuthoritySet;

	/// Update the current GRANDPA authority set.
	///
	/// Should only be updated when a scheduled change has been triggered.
	fn update_current_authority_set(&self, new_set: AuthoritySet);

	/// Replace the current authority set with the next scheduled set.
	///
	/// Returns an error if there is no scheduled authority set to enact.
	#[allow(clippy::result_unit_err)]
	fn enact_authority_set(&mut self, signal_hash: <Self::Header as HeaderT>::Hash) -> Result<(), ()>;

	/// Get the next scheduled GRANDPA authority set change.
	fn scheduled_set_change(
		&self,
		signal_hash: <Self::Header as HeaderT>::Hash,
	) -> Option<ScheduledChange<<Self::Header as HeaderT>::Number>>;

	/// Schedule a GRANDPA authority set change in the future.
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

impl<T: Config> BridgeStorage for PalletStorage<T> {
	type Header = BridgedHeader<T>;

	fn write_header(&mut self, header: &ImportedHeader<BridgedHeader<T>>) {
		use core::cmp::Ordering;

		let hash = header.hash();
		let current_height = header.number();
		let best_height = <BestHeight<T>>::get();

		match current_height.cmp(&best_height) {
			Ordering::Equal => {
				// Want to avoid duplicates in the case where we're writing a finalized header to
				// storage which also happens to be at the best height the best height
				let not_duplicate = !<ImportedHeaders<T>>::contains_key(hash);
				if not_duplicate {
					<BestHeaders<T>>::append(hash);
				}
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
		// We will only construct a dummy header if the pallet is not initialized and someone tries
		// to use the public module interface (not dispatchables) to get the best finalized header.
		// This is an edge case since this can only really happen when bootstrapping the bridge.
		let hash = <BestFinalized<T>>::get();
		self.header_by_hash(hash).unwrap_or_else(|| ImportedHeader {
			header: <BridgedHeader<T>>::new(
				Default::default(),
				Default::default(),
				Default::default(),
				Default::default(),
				Default::default(),
			),
			requires_justification: false,
			is_finalized: false,
			signal_hash: None,
		})
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
	use crate::mock::{run_test, test_header, unfinalized_header, Origin, TestHeader, TestRuntime};
	use bp_test_utils::authority_list;
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::DispatchError;

	fn init_with_origin(origin: Origin) -> Result<InitializationData<TestHeader>, DispatchError> {
		let init_data = InitializationData {
			header: test_header(1),
			authority_list: authority_list(),
			set_id: 1,
			scheduled_change: None,
			is_halted: false,
		};

		Module::<TestRuntime>::initialize(origin, init_data.clone()).map(|_| init_data)
	}

	#[test]
	fn init_root_or_owner_origin_can_initialize_pallet() {
		run_test(|| {
			assert_noop!(init_with_origin(Origin::signed(1)), DispatchError::BadOrigin);
			assert_ok!(init_with_origin(Origin::root()));

			// Reset storage so we can initialize the pallet again
			BestFinalized::<TestRuntime>::kill();
			ModuleOwner::<TestRuntime>::put(2);
			assert_ok!(init_with_origin(Origin::signed(2)));
		})
	}

	#[test]
	fn init_storage_entries_are_correctly_initialized() {
		run_test(|| {
			assert!(Module::<TestRuntime>::best_headers().is_empty());
			assert_eq!(Module::<TestRuntime>::best_finalized(), test_header(0));

			let init_data = init_with_origin(Origin::root()).unwrap();

			let storage = PalletStorage::<TestRuntime>::new();
			assert!(storage.header_exists(init_data.header.hash()));
			assert_eq!(
				storage.best_headers()[0],
				crate::HeaderId {
					number: *init_data.header.number(),
					hash: init_data.header.hash()
				}
			);
			assert_eq!(storage.best_finalized_header().hash(), init_data.header.hash());
			assert_eq!(storage.current_authority_set().authorities, init_data.authority_list);
			assert_eq!(IsHalted::get(), false);
		})
	}

	#[test]
	fn init_can_only_initialize_pallet_once() {
		run_test(|| {
			assert_ok!(init_with_origin(Origin::root()));
			assert_noop!(
				init_with_origin(Origin::root()),
				<Error<TestRuntime>>::AlreadyInitialized
			);
		})
	}

	#[test]
	fn pallet_owner_may_change_owner() {
		run_test(|| {
			ModuleOwner::<TestRuntime>::put(2);

			assert_ok!(Module::<TestRuntime>::set_owner(Origin::root(), Some(1)));
			assert_noop!(
				Module::<TestRuntime>::halt_operations(Origin::signed(2)),
				DispatchError::BadOrigin,
			);
			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::root()));

			assert_ok!(Module::<TestRuntime>::set_owner(Origin::signed(1), None));
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(2)),
				DispatchError::BadOrigin,
			);
			assert_ok!(Module::<TestRuntime>::resume_operations(Origin::root()));
		});
	}

	#[test]
	fn pallet_may_be_halted_by_root() {
		run_test(|| {
			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::root()));
			assert_ok!(Module::<TestRuntime>::resume_operations(Origin::root()));
		});
	}

	#[test]
	fn pallet_may_be_halted_by_owner() {
		run_test(|| {
			ModuleOwner::<TestRuntime>::put(2);

			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::signed(2)));
			assert_ok!(Module::<TestRuntime>::resume_operations(Origin::signed(2)));

			assert_noop!(
				Module::<TestRuntime>::halt_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);

			assert_ok!(Module::<TestRuntime>::halt_operations(Origin::signed(2)));
			assert_noop!(
				Module::<TestRuntime>::resume_operations(Origin::signed(1)),
				DispatchError::BadOrigin,
			);
		});
	}

	#[test]
	fn pallet_rejects_transactions_if_halted() {
		run_test(|| {
			IsHalted::put(true);

			assert_noop!(
				Module::<TestRuntime>::import_signed_header(Origin::signed(1), test_header(1)),
				Error::<TestRuntime>::Halted,
			);

			assert_noop!(
				Module::<TestRuntime>::finalize_header(Origin::signed(1), test_header(1).hash(), vec![]),
				Error::<TestRuntime>::Halted,
			);
		})
	}

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
