// Copyright 2021 Parity Technologies (UK) Ltd.
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

//! Substrate Finality Verifier Pallet
//!
//! The goal of this pallet is to provide a safe interface for writing finalized headers to an
//! external pallet which tracks headers and finality proofs. By safe, we mean that only headers
//! whose finality has been verified will be written to the underlying pallet.
//!
//! By verifying the finality of headers before writing them to storage we prevent DoS vectors in
//! which unfinalized headers get written to storage even if they don't have a chance of being
//! finalized in the future (such as in the case where a different fork gets finalized).
//!
//! The underlying pallet used for storage is assumed to be a pallet which tracks headers and
//! GRANDPA authority set changes. This information is used during the verification of GRANDPA
//! finality proofs.

#![cfg_attr(not(feature = "std"), no_std)]
// Runtime-generated enums
#![allow(clippy::large_enum_variant)]

use bp_header_chain::{justification::verify_justification, AncestryChecker, HeaderChain};
use bp_runtime::{BlockNumberOf, Chain, HashOf, HasherOf, HeaderOf};
use codec::{Decode, Encode};
use finality_grandpa::voter_set::VoterSet;
use frame_support::{dispatch::DispatchError, ensure};
use frame_system::{ensure_signed, RawOrigin};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_finality_grandpa::{ConsensusLog, GRANDPA_ENGINE_ID};
use sp_runtime::traits::{BadOrigin, Header as HeaderT, Zero};
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;

// Re-export in crate namespace for `construct_runtime!`
pub use pallet::*;

/// Block number of the bridged chain.
pub type BridgedBlockNumber<T> = BlockNumberOf<<T as Config>::BridgedChain>;
/// Block hash of the bridged chain.
pub type BridgedBlockHash<T> = HashOf<<T as Config>::BridgedChain>;
/// Hasher of the bridged chain.
pub type _BridgedBlockHasher<T> = HasherOf<<T as Config>::BridgedChain>;
/// Header of the bridged chain.
pub type BridgedHeader<T> = HeaderOf<<T as Config>::BridgedChain>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The chain we are bridging to here.
		type BridgedChain: Chain;

		/// The pallet which we will use as our underlying storage mechanism.
		type HeaderChain: HeaderChain<<Self::BridgedChain as Chain>::Header, DispatchError>;

		/// The type of ancestry proof used by the pallet.
		///
		/// Will be used by the ancestry checker to verify that the header being finalized is
		/// related to the best finalized header in storage.
		type AncestryProof: Parameter;

		/// The type through which we will verify that a given header is related to the last
		/// finalized header in our storage pallet.
		type AncestryChecker: AncestryChecker<<Self::BridgedChain as Chain>::Header, Self::AncestryProof>;

		/// The upper bound on the number of requests allowed by the pallet.
		///
		/// A request refers to an action which writes a header to storage.
		///
		/// Once this bound is reached the pallet will not allow any dispatchables to be called
		/// until the request count has decreased.
		#[pallet::constant]
		type MaxRequests: Get<u32>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: T::BlockNumber) -> frame_support::weights::Weight {
			<RequestCount<T>>::mutate(|count| *count = count.saturating_sub(1));

			(0_u64)
				.saturating_add(T::DbWeight::get().reads(1))
				.saturating_add(T::DbWeight::get().writes(1))
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Verify a target header is finalized according to the given finality proof.
		///
		/// It will use the underlying storage pallet to fetch information about the current
		/// authorities and best finalized header in order to verify that the header is finalized.
		///
		/// If successful in verification, it will write the target header to the underlying storage
		/// pallet.
		#[pallet::weight(0)]
		pub fn submit_finality_proof(
			origin: OriginFor<T>,
			finality_target: BridgedHeader<T>,
			justification: Vec<u8>,
			ancestry_proof: T::AncestryProof,
		) -> DispatchResultWithPostInfo {
			ensure_operational::<T>()?;
			let _ = ensure_signed(origin)?;

			ensure!(
				Self::request_count() < T::MaxRequests::get(),
				<Error<T>>::TooManyRequests
			);

			let (hash, number) = (finality_target.hash(), finality_target.number());
			frame_support::debug::trace!("Going to try and finalize header {:?}", finality_target);

			let best_finalized = <ImportedHeaders<T>>::get(<BestFinalized<T>>::get()).expect(
				"In order to reach this point the bridge must have been initialized. Afterwards,
				every time `BestFinalized` is updated `ImportedHeaders` is also updated. Therefore
				`ImportedHeaders` must contain an entry for `BestFinalized`.",
			);

			// We do a quick check here to ensure that our header chain is making progress and isn't
			// "travelling back in time" (which could be indicative of something bad, e.g a hard-fork).
			ensure!(best_finalized.number() < number, <Error<T>>::OldHeader);

			let authority_set = <CurrentAuthoritySet<T>>::get();
			let voter_set = VoterSet::new(authority_set.authorities).ok_or(<Error<T>>::InvalidAuthoritySet)?;
			let set_id = authority_set.set_id;

			verify_justification::<BridgedHeader<T>>((hash, *number), set_id, voter_set, &justification).map_err(
				|e| {
					frame_support::debug::error!("Received invalid justification for {:?}: {:?}", finality_target, e);
					<Error<T>>::InvalidJustification
				},
			)?;

			let best_finalized = T::HeaderChain::best_finalized();
			frame_support::debug::trace!("Checking ancestry against best finalized header: {:?}", &best_finalized);

			ensure!(
				T::AncestryChecker::are_ancestors(&best_finalized, &finality_target, &ancestry_proof),
				<Error<T>>::InvalidAncestryProof
			);

			let _ = T::HeaderChain::append_header(finality_target.clone())?;

			import_header::<T>(hash, finality_target)?;
			<RequestCount<T>>::mutate(|count| *count += 1);

			frame_support::debug::info!("Succesfully imported finalized header with hash {:?}!", hash);

			Ok(().into())
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
		#[pallet::weight((T::DbWeight::get().reads_writes(2, 5), DispatchClass::Operational))]
		pub fn initialize(
			origin: OriginFor<T>,
			init_data: super::InitializationData<BridgedHeader<T>>,
		) -> DispatchResultWithPostInfo {
			ensure_owner_or_root::<T>(origin)?;

			let init_allowed = !<BestFinalized<T>>::exists();
			ensure!(init_allowed, <Error<T>>::AlreadyInitialized);
			initialize_bridge::<T>(init_data.clone());

			frame_support::debug::info!(
				"Pallet has been initialized with the following parameters: {:?}",
				init_data
			);

			Ok(().into())
		}

		/// Change `ModuleOwner`.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_owner(origin: OriginFor<T>, new_owner: Option<T::AccountId>) -> DispatchResultWithPostInfo {
			ensure_owner_or_root::<T>(origin)?;
			match new_owner {
				Some(new_owner) => {
					ModuleOwner::<T>::put(&new_owner);
					frame_support::debug::info!("Setting pallet Owner to: {:?}", new_owner);
				}
				None => {
					ModuleOwner::<T>::kill();
					frame_support::debug::info!("Removed Owner of pallet.");
				}
			}

			Ok(().into())
		}

		/// Halt all pallet operations. Operations may be resumed using `resume_operations` call.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn halt_operations(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_owner_or_root::<T>(origin)?;
			<IsHalted<T>>::put(true);
			frame_support::debug::warn!("Stopping pallet operations.");

			Ok(().into())
		}

		/// Resume all pallet operations. May be called even if pallet is halted.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn resume_operations(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_owner_or_root::<T>(origin)?;
			<IsHalted<T>>::put(false);
			frame_support::debug::info!("Resuming pallet operations.");

			Ok(().into())
		}
	}

	/// The current number of requests which have written to storage.
	///
	/// If the `RequestCount` hits `MaxRequests`, no more calls will be allowed to the pallet until
	/// the request capacity is increased.
	///
	/// The `RequestCount` is decreased by one at the beginning of every block. This is to ensure
	/// that the pallet can always make progress.
	#[pallet::storage]
	#[pallet::getter(fn request_count)]
	pub(super) type RequestCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Hash of the header used to bootstrap the pallet.
	#[pallet::storage]
	pub(super) type InitialHash<T: Config> = StorageValue<_, BridgedBlockHash<T>, ValueQuery>;

	/// Hash of the best finalized header.
	#[pallet::storage]
	pub(super) type BestFinalized<T: Config> = StorageValue<_, BridgedBlockHash<T>, ValueQuery>;

	/// Headers which have been imported into the pallet.
	#[pallet::storage]
	pub(super) type ImportedHeaders<T: Config> = StorageMap<_, Identity, BridgedBlockHash<T>, BridgedHeader<T>>;

	/// The current GRANDPA Authority set.
	#[pallet::storage]
	pub(super) type CurrentAuthoritySet<T: Config> = StorageValue<_, bp_header_chain::AuthoritySet, ValueQuery>;

	/// Optional pallet owner.
	///
	/// Pallet owner has a right to halt all pallet operations and then resume it. If it is
	/// `None`, then there are no direct ways to halt/resume pallet operations, but other
	/// runtime methods may still be used to do that (i.e. democracy::referendum to update halt
	/// flag directly or call the `halt_operations`).
	#[pallet::storage]
	pub(super) type ModuleOwner<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	/// If true, all pallet transactions are failed immediately.
	#[pallet::storage]
	pub(super) type IsHalted<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		owner: Option<T::AccountId>,
		init_data: Option<super::InitializationData<BridgedHeader<T>>>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				owner: None,
				init_data: None,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			if let Some(ref owner) = self.owner {
				<ModuleOwner<T>>::put(owner);
			}

			if let Some(init_data) = self.init_data.clone() {
				initialize_bridge::<T>(init_data);
			} else {
				// Since the bridge hasn't been initialized we shouldn't allow anyone to perform
				// transactions.
				<IsHalted<T>>::put(true);
			}
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The given justification is invalid for the given header.
		InvalidJustification,
		/// The given ancestry proof is unable to verify that the child and ancestor headers are
		/// related.
		InvalidAncestryProof,
		/// The authority set from the underlying header chain is invalid.
		InvalidAuthoritySet,
		/// Failed to write a header to the underlying header chain.
		FailedToWriteHeader,
		/// There are too many requests for the current window to handle.
		TooManyRequests,
		/// The header being imported is older than the best finalized header known to the pallet.
		OldHeader,
		/// The scheduled authority set change found in the header is unsupported by the pallet.
		///
		/// This is the case for non-standard (e.g forced) authority set changes.
		UnsupportedScheduledChange,
		/// The pallet has already been initialized.
		AlreadyInitialized,
		/// All pallet operations are halted.
		Halted,
	}

	/// Import the given header to the pallet's storage.
	///
	/// This function will also check if the header schedules and enacts authority set changes,
	/// updating the current authority set accordingly.
	///
	/// Note: This function assumes that the given header has already been proven to be valid and
	/// finalized. Using this assumption it will write them to storage with minimal checks. That
	/// means it's of great importance that this function *not* called with any headers whose
	/// finality has not been checked, otherwise you risk bricking your bridge.
	pub(crate) fn import_header<T: Config>(
		hash: BridgedBlockHash<T>,
		header: BridgedHeader<T>,
	) -> Result<(), sp_runtime::DispatchError> {
		// We don't support forced changes - at that point governance intervention is required.
		ensure!(
			super::find_forced_change(&header).is_none(),
			<Error<T>>::UnsupportedScheduledChange
		);

		if let Some(change) = super::find_scheduled_change(&header) {
			// GRANDPA only includes a `delay` for forced changes, so this isn't valid.
			ensure!(change.delay == Zero::zero(), <Error<T>>::UnsupportedScheduledChange);

			// TODO [#788]: Stop manually increasing the `set_id` here.
			let next_authorities = bp_header_chain::AuthoritySet {
				authorities: change.next_authorities,
				set_id: <CurrentAuthoritySet<T>>::get().set_id + 1,
			};

			// Since our header schedules a change and we know the delay is 0, it must also enact
			// the change.
			<CurrentAuthoritySet<T>>::put(next_authorities);
		};

		<BestFinalized<T>>::put(hash);
		<ImportedHeaders<T>>::insert(hash, header);

		Ok(())
	}

	/// Since this writes to storage with no real checks this should only be used in functions that
	/// were called by a trusted origin.
	fn initialize_bridge<T: Config>(init_params: super::InitializationData<BridgedHeader<T>>) {
		let super::InitializationData {
			header,
			authority_list,
			set_id,
			is_halted,
		} = init_params;

		let initial_hash = header.hash();
		<InitialHash<T>>::put(initial_hash);
		<BestFinalized<T>>::put(initial_hash);
		<ImportedHeaders<T>>::insert(initial_hash, header);

		let authority_set = bp_header_chain::AuthoritySet::new(authority_list, set_id);
		<CurrentAuthoritySet<T>>::put(authority_set);

		<IsHalted<T>>::put(is_halted);
	}

	/// Ensure that the origin is either root, or `ModuleOwner`.
	fn ensure_owner_or_root<T: Config>(origin: T::Origin) -> Result<(), BadOrigin> {
		match origin.into() {
			Ok(RawOrigin::Root) => Ok(()),
			Ok(RawOrigin::Signed(ref signer)) if Some(signer) == <ModuleOwner<T>>::get().as_ref() => Ok(()),
			_ => Err(BadOrigin),
		}
	}

	/// Ensure that the pallet is in operational mode (not halted).
	fn ensure_operational<T: Config>() -> Result<(), Error<T>> {
		if <IsHalted<T>>::get() {
			Err(<Error<T>>::Halted)
		} else {
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Get the best finalized header the pallet knows of.
	///
	/// Returns a dummy header if there is no best header. This can only happen
	/// if the pallet has not been initialized yet.
	pub fn best_finalized() -> BridgedHeader<T> {
		let hash = <BestFinalized<T>>::get();
		<ImportedHeaders<T>>::get(hash).unwrap_or_else(|| {
			<BridgedHeader<T>>::new(
				Default::default(),
				Default::default(),
				Default::default(),
				Default::default(),
				Default::default(),
			)
		})
	}

	/// Check if a particular header is known to the bridge pallet.
	pub fn is_known_header(hash: BridgedBlockHash<T>) -> bool {
		<ImportedHeaders<T>>::contains_key(hash)
	}
}

/// Data required for initializing the bridge pallet.
///
/// The bridge needs to know where to start its sync from, and this provides that initial context.
#[derive(Default, Encode, Decode, RuntimeDebug, PartialEq, Clone)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct InitializationData<H: HeaderT> {
	/// The header from which we should start syncing.
	pub header: H,
	/// The initial authorities of the pallet.
	pub authority_list: sp_finality_grandpa::AuthorityList,
	/// The ID of the initial authority set.
	pub set_id: sp_finality_grandpa::SetId,
	/// Should the pallet block transaction immediately after initialization.
	pub is_halted: bool,
}

pub(crate) fn find_scheduled_change<H: HeaderT>(header: &H) -> Option<sp_finality_grandpa::ScheduledChange<H::Number>> {
	use sp_runtime::generic::OpaqueDigestItemId;

	let id = OpaqueDigestItemId::Consensus(&GRANDPA_ENGINE_ID);

	let filter_log = |log: ConsensusLog<H::Number>| match log {
		ConsensusLog::ScheduledChange(change) => Some(change),
		_ => None,
	};

	// find the first consensus digest with the right ID which converts to
	// the right kind of consensus log.
	header.digest().convert_first(|l| l.try_to(id).and_then(filter_log))
}

/// Checks the given header for a consensus digest signalling a **forced** scheduled change and
/// extracts it.
pub(crate) fn find_forced_change<H: HeaderT>(
	header: &H,
) -> Option<(H::Number, sp_finality_grandpa::ScheduledChange<H::Number>)> {
	use sp_runtime::generic::OpaqueDigestItemId;

	let id = OpaqueDigestItemId::Consensus(&GRANDPA_ENGINE_ID);

	let filter_log = |log: ConsensusLog<H::Number>| match log {
		ConsensusLog::ForcedChange(delay, change) => Some((delay, change)),
		_ => None,
	};

	// find the first consensus digest with the right ID which converts to
	// the right kind of consensus log.
	header.digest().convert_first(|l| l.try_to(id).and_then(filter_log))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{run_test, test_header, Origin, TestHash, TestHeader, TestNumber, TestRuntime};
	use bp_test_utils::{alice, authority_list, bob, make_justification_for_header};
	use codec::Encode;
	use frame_support::weights::PostDispatchInfo;
	use frame_support::{assert_err, assert_noop, assert_ok};
	use sp_runtime::{Digest, DigestItem, DispatchError};

	fn initialize_substrate_bridge() {
		assert_ok!(init_with_origin(Origin::root()));
	}

	fn init_with_origin(
		origin: Origin,
	) -> Result<InitializationData<TestHeader>, sp_runtime::DispatchErrorWithPostInfo<PostDispatchInfo>> {
		let genesis = test_header(0);

		let init_data = InitializationData {
			header: genesis,
			authority_list: authority_list(),
			set_id: 1,
			is_halted: false,
		};

		Module::<TestRuntime>::initialize(origin, init_data.clone()).map(|_| init_data)
	}

	fn submit_finality_proof(child: u8, header: u8) -> frame_support::dispatch::DispatchResultWithPostInfo {
		let child = test_header(child.into());
		let header = test_header(header.into());

		let set_id = 1;
		let grandpa_round = 1;
		let justification = make_justification_for_header(&header, grandpa_round, set_id, &authority_list()).encode();
		let ancestry_proof = vec![child, header.clone()];

		Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof)
	}

	fn next_block() {
		use frame_support::traits::OnInitialize;

		let current_number = frame_system::Module::<TestRuntime>::block_number();
		frame_system::Module::<TestRuntime>::set_block_number(current_number + 1);
		let _ = Module::<TestRuntime>::on_initialize(current_number);
	}

	fn change_log(delay: u64) -> Digest<TestHash> {
		let consensus_log = ConsensusLog::<TestNumber>::ScheduledChange(sp_finality_grandpa::ScheduledChange {
			next_authorities: vec![(alice(), 1), (bob(), 1)],
			delay,
		});

		Digest::<TestHash> {
			logs: vec![DigestItem::Consensus(GRANDPA_ENGINE_ID, consensus_log.encode())],
		}
	}

	fn forced_change_log(delay: u64) -> Digest<TestHash> {
		let consensus_log = ConsensusLog::<TestNumber>::ForcedChange(
			delay,
			sp_finality_grandpa::ScheduledChange {
				next_authorities: vec![(alice(), 1), (bob(), 1)],
				delay,
			},
		);

		Digest::<TestHash> {
			logs: vec![DigestItem::Consensus(GRANDPA_ENGINE_ID, consensus_log.encode())],
		}
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
			assert_eq!(
				BestFinalized::<TestRuntime>::get(),
				BridgedBlockHash::<TestRuntime>::default()
			);
			assert_eq!(Module::<TestRuntime>::best_finalized(), test_header(0));

			let init_data = init_with_origin(Origin::root()).unwrap();

			assert!(<ImportedHeaders<TestRuntime>>::contains_key(init_data.header.hash()));
			assert_eq!(BestFinalized::<TestRuntime>::get(), init_data.header.hash());
			assert_eq!(
				CurrentAuthoritySet::<TestRuntime>::get().authorities,
				init_data.authority_list
			);
			assert_eq!(IsHalted::<TestRuntime>::get(), false);
		})
	}

	#[test]
	fn init_can_only_initialize_pallet_once() {
		run_test(|| {
			initialize_substrate_bridge();
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
			<IsHalted<TestRuntime>>::put(true);

			assert_noop!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), test_header(1), vec![], vec![]),
				Error::<TestRuntime>::Halted,
			);
		})
	}

	#[test]
	fn succesfully_imports_header_with_valid_finality_and_ancestry_proofs() {
		run_test(|| {
			initialize_substrate_bridge();

			assert_ok!(submit_finality_proof(1, 2));

			let header = test_header(2);
			assert_eq!(<BestFinalized<TestRuntime>>::get(), header.hash());
			assert!(<ImportedHeaders<TestRuntime>>::contains_key(header.hash()));
		})
	}

	#[test]
	fn rejects_justification_that_skips_authority_set_transition() {
		run_test(|| {
			initialize_substrate_bridge();

			let child = test_header(1);
			let header = test_header(2);

			let set_id = 2;
			let grandpa_round = 1;
			let justification =
				make_justification_for_header(&header, grandpa_round, set_id, &authority_list()).encode();
			let ancestry_proof = vec![child, header.clone()];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidJustification
			);
		})
	}

	#[test]
	fn does_not_import_header_with_invalid_finality_proof() {
		run_test(|| {
			initialize_substrate_bridge();

			let child = test_header(1);
			let header = test_header(2);

			let justification = [1u8; 32].encode();
			let ancestry_proof = vec![child, header.clone()];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidJustification
			);
		})
	}

	#[test]
	fn does_not_import_header_with_invalid_ancestry_proof() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(2);

			let set_id = 1;
			let grandpa_round = 1;
			let justification =
				make_justification_for_header(&header, grandpa_round, set_id, &authority_list()).encode();

			// For testing, we've made it so that an empty ancestry proof is invalid
			let ancestry_proof = vec![];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidAncestryProof
			);
		})
	}

	#[test]
	fn disallows_invalid_authority_set() {
		run_test(|| {
			use bp_test_utils::{alice, bob};

			let genesis = test_header(0);

			let invalid_authority_list = vec![(alice(), u64::MAX), (bob(), u64::MAX)];
			let init_data = InitializationData {
				header: genesis,
				authority_list: invalid_authority_list,
				set_id: 1,
				is_halted: false,
			};

			assert_ok!(Module::<TestRuntime>::initialize(Origin::root(), init_data));

			let header = test_header(1);
			let justification = [1u8; 32].encode();
			let ancestry_proof = vec![];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidAuthoritySet
			);
		})
	}

	#[test]
	fn importing_header_ensures_that_chain_is_extended() {
		run_test(|| {
			initialize_substrate_bridge();

			assert_ok!(submit_finality_proof(5, 6));
			assert_err!(submit_finality_proof(3, 4), Error::<TestRuntime>::OldHeader);
			assert_ok!(submit_finality_proof(7, 8));
		})
	}

	#[test]
	fn importing_header_enacts_new_authority_set() {
		run_test(|| {
			initialize_substrate_bridge();

			let next_set_id = 2;
			let next_authorities = vec![(alice(), 1), (bob(), 1)];

			// Need to update the header digest to indicate that our header signals an authority set
			// change. The change will be enacted when we import our header.
			let mut header = test_header(2);
			header.digest = change_log(0);

			// Let's import our test header
			assert_ok!(pallet::import_header::<TestRuntime>(header.hash(), header.clone()));

			// Make sure that our header is the best finalized
			assert_eq!(<BestFinalized<TestRuntime>>::get(), header.hash());
			assert!(<ImportedHeaders<TestRuntime>>::contains_key(header.hash()));

			// Make sure that the authority set actually changed upon importing our header
			assert_eq!(
				<CurrentAuthoritySet<TestRuntime>>::get(),
				bp_header_chain::AuthoritySet::new(next_authorities, next_set_id),
			);
		})
	}

	#[test]
	fn importing_header_rejects_header_with_scheduled_change_delay() {
		run_test(|| {
			initialize_substrate_bridge();

			// Need to update the header digest to indicate that our header signals an authority set
			// change. However, the change doesn't happen until the next block.
			let mut header = test_header(2);
			header.digest = change_log(1);

			// Should not be allowed to import this header
			assert_err!(
				pallet::import_header::<TestRuntime>(header.hash(), header),
				<Error<TestRuntime>>::UnsupportedScheduledChange
			);
		})
	}

	#[test]
	fn importing_header_rejects_header_with_forced_changes() {
		run_test(|| {
			initialize_substrate_bridge();

			// Need to update the header digest to indicate that it signals a forced authority set
			// change.
			let mut header = test_header(2);
			header.digest = forced_change_log(0);

			// Should not be allowed to import this header
			assert_err!(
				pallet::import_header::<TestRuntime>(header.hash(), header),
				<Error<TestRuntime>>::UnsupportedScheduledChange
			);
		})
	}
	#[test]
	fn rate_limiter_disallows_imports_once_limit_is_hit_in_single_block() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1, 2));
			assert_ok!(submit_finality_proof(3, 4));
			assert_err!(submit_finality_proof(5, 6), <Error<TestRuntime>>::TooManyRequests);
		})
	}

	#[test]
	fn rate_limiter_invalid_requests_do_not_count_towards_request_count() {
		run_test(|| {
			let submit_invalid_request = || {
				let child = test_header(1);
				let header = test_header(2);

				let invalid_justification = vec![4, 2, 4, 2].encode();
				let ancestry_proof = vec![child, header.clone()];

				Module::<TestRuntime>::submit_finality_proof(
					Origin::signed(1),
					header,
					invalid_justification,
					ancestry_proof,
				)
			};

			initialize_substrate_bridge();

			for _ in 0..<TestRuntime as Config>::MaxRequests::get() + 1 {
				// Notice that the error here *isn't* `TooManyRequests`
				assert_err!(submit_invalid_request(), <Error<TestRuntime>>::InvalidJustification);
			}

			// Can still submit `MaxRequests` requests afterwards
			assert_ok!(submit_finality_proof(1, 2));
			assert_ok!(submit_finality_proof(3, 4));
			assert_err!(submit_finality_proof(5, 6), <Error<TestRuntime>>::TooManyRequests);
		})
	}

	#[test]
	fn rate_limiter_allows_request_after_new_block_has_started() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1, 2));
			assert_ok!(submit_finality_proof(3, 4));

			next_block();
			assert_ok!(submit_finality_proof(5, 6));
		})
	}

	#[test]
	fn rate_limiter_disallows_imports_once_limit_is_hit_across_different_blocks() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1, 2));
			assert_ok!(submit_finality_proof(3, 4));

			next_block();
			assert_ok!(submit_finality_proof(5, 6));
			assert_err!(submit_finality_proof(7, 8), <Error<TestRuntime>>::TooManyRequests);
		})
	}

	#[test]
	fn rate_limiter_allows_max_requests_after_long_time_with_no_activity() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1, 2));
			assert_ok!(submit_finality_proof(3, 4));

			next_block();
			next_block();

			next_block();
			assert_ok!(submit_finality_proof(5, 6));
			assert_ok!(submit_finality_proof(7, 8));
		})
	}
}
