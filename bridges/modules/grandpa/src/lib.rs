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

//! Substrate GRANDPA Pallet
//!
//! This pallet is an on-chain GRANDPA light client for Substrate based chains.
//!
//! This pallet achieves this by trustlessly verifying GRANDPA finality proofs on-chain. Once
//! verified, finalized headers are stored in the pallet, thereby creating a sparse header chain.
//! This sparse header chain can be used as a source of truth for other higher-level applications.
//!
//! The pallet is responsible for tracking GRANDPA validator set hand-offs. We only import headers
//! with justifications signed by the current validator set we know of. The header is inspected for
//! a `ScheduledChanges` digest item, which is then used to update to next validator set.
//!
//! Since this pallet only tracks finalized headers it does not deal with forks. Forks can only
//! occur if the GRANDPA validator set on the bridged chain is either colluding or there is a severe
//! bug causing resulting in an equivocation. Such events are outside of the scope of this pallet.
//! Shall the fork occur on the bridged chain governance intervention will be required to
//! re-initialize the bridge and track the right fork.

#![cfg_attr(not(feature = "std"), no_std)]
// Runtime-generated enums
#![allow(clippy::large_enum_variant)]

use bp_runtime::{BlockNumberOf, Chain, HashOf, HasherOf, HeaderOf};
use codec::{Decode, Encode};
use finality_grandpa::voter_set::VoterSet;
use frame_support::ensure;
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
pub type BridgedBlockNumber<T, I> = BlockNumberOf<<T as Config<I>>::BridgedChain>;
/// Block hash of the bridged chain.
pub type BridgedBlockHash<T, I> = HashOf<<T as Config<I>>::BridgedChain>;
/// Hasher of the bridged chain.
pub type BridgedBlockHasher<T, I> = HasherOf<<T as Config<I>>::BridgedChain>;
/// Header of the bridged chain.
pub type BridgedHeader<T, I> = HeaderOf<<T as Config<I>>::BridgedChain>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The chain we are bridging to here.
		type BridgedChain: Chain;

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
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_initialize(_n: T::BlockNumber) -> frame_support::weights::Weight {
			<RequestCount<T, I>>::mutate(|count| *count = count.saturating_sub(1));

			(0_u64)
				.saturating_add(T::DbWeight::get().reads(1))
				.saturating_add(T::DbWeight::get().writes(1))
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
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
			finality_target: BridgedHeader<T, I>,
			justification: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			ensure_operational::<T, I>()?;
			let _ = ensure_signed(origin)?;

			ensure!(
				Self::request_count() < T::MaxRequests::get(),
				<Error<T, I>>::TooManyRequests
			);

			let (hash, number) = (finality_target.hash(), finality_target.number());
			log::trace!(target: "runtime::bridge-grandpa", "Going to try and finalize header {:?}", finality_target);

			let best_finalized = <ImportedHeaders<T, I>>::get(<BestFinalized<T, I>>::get()).expect(
				"In order to reach this point the bridge must have been initialized. Afterwards,
				every time `BestFinalized` is updated `ImportedHeaders` is also updated. Therefore
				`ImportedHeaders` must contain an entry for `BestFinalized`.",
			);

			// We do a quick check here to ensure that our header chain is making progress and isn't
			// "travelling back in time" (which could be indicative of something bad, e.g a hard-fork).
			ensure!(best_finalized.number() < number, <Error<T, I>>::OldHeader);

			verify_justification::<T, I>(&justification, hash, *number)?;

			try_enact_authority_change::<T, I>(&finality_target)?;
			<BestFinalized<T, I>>::put(hash);
			<ImportedHeaders<T, I>>::insert(hash, finality_target);
			<RequestCount<T, I>>::mutate(|count| *count += 1);

			log::info!(target: "runtime::bridge-grandpa", "Succesfully imported finalized header with hash {:?}!", hash);

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
			init_data: super::InitializationData<BridgedHeader<T, I>>,
		) -> DispatchResultWithPostInfo {
			ensure_owner_or_root::<T, I>(origin)?;

			let init_allowed = !<BestFinalized<T, I>>::exists();
			ensure!(init_allowed, <Error<T, I>>::AlreadyInitialized);
			initialize_bridge::<T, I>(init_data.clone());

			log::info!(
				target: "runtime::bridge-grandpa",
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
			ensure_owner_or_root::<T, I>(origin)?;
			match new_owner {
				Some(new_owner) => {
					ModuleOwner::<T, I>::put(&new_owner);
					log::info!(target: "runtime::bridge-grandpa", "Setting pallet Owner to: {:?}", new_owner);
				}
				None => {
					ModuleOwner::<T, I>::kill();
					log::info!(target: "runtime::bridge-grandpa", "Removed Owner of pallet.");
				}
			}

			Ok(().into())
		}

		/// Halt or resume all pallet operations.
		///
		/// May only be called either by root, or by `ModuleOwner`.
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operational(origin: OriginFor<T>, operational: bool) -> DispatchResultWithPostInfo {
			ensure_owner_or_root::<T, I>(origin)?;
			<IsHalted<T, I>>::put(operational);

			if operational {
				log::info!(target: "runtime::bridge-grandpa", "Resuming pallet operations.");
			} else {
				log::warn!(target: "runtime::bridge-grandpa", "Stopping pallet operations.");
			}

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
	pub(super) type RequestCount<T: Config<I>, I: 'static = ()> = StorageValue<_, u32, ValueQuery>;

	/// Hash of the header used to bootstrap the pallet.
	#[pallet::storage]
	pub(super) type InitialHash<T: Config<I>, I: 'static = ()> = StorageValue<_, BridgedBlockHash<T, I>, ValueQuery>;

	/// Hash of the best finalized header.
	#[pallet::storage]
	pub(super) type BestFinalized<T: Config<I>, I: 'static = ()> = StorageValue<_, BridgedBlockHash<T, I>, ValueQuery>;

	/// Headers which have been imported into the pallet.
	#[pallet::storage]
	pub(super) type ImportedHeaders<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, BridgedBlockHash<T, I>, BridgedHeader<T, I>>;

	/// The current GRANDPA Authority set.
	#[pallet::storage]
	pub(super) type CurrentAuthoritySet<T: Config<I>, I: 'static = ()> =
		StorageValue<_, bp_header_chain::AuthoritySet, ValueQuery>;

	/// Optional pallet owner.
	///
	/// Pallet owner has a right to halt all pallet operations and then resume it. If it is
	/// `None`, then there are no direct ways to halt/resume pallet operations, but other
	/// runtime methods may still be used to do that (i.e. democracy::referendum to update halt
	/// flag directly or call the `halt_operations`).
	#[pallet::storage]
	pub(super) type ModuleOwner<T: Config<I>, I: 'static = ()> = StorageValue<_, T::AccountId, OptionQuery>;

	/// If true, all pallet transactions are failed immediately.
	#[pallet::storage]
	pub(super) type IsHalted<T: Config<I>, I: 'static = ()> = StorageValue<_, bool, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		/// Optional module owner account.
		pub owner: Option<T::AccountId>,
		/// Optional module initialization data.
		pub init_data: Option<super::InitializationData<BridgedHeader<T, I>>>,
	}

	#[cfg(feature = "std")]
	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self {
				owner: None,
				init_data: None,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> GenesisBuild<T, I> for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(ref owner) = self.owner {
				<ModuleOwner<T, I>>::put(owner);
			}

			if let Some(init_data) = self.init_data.clone() {
				initialize_bridge::<T, I>(init_data);
			} else {
				// Since the bridge hasn't been initialized we shouldn't allow anyone to perform
				// transactions.
				<IsHalted<T, I>>::put(true);
			}
		}
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The given justification is invalid for the given header.
		InvalidJustification,
		/// The authority set from the underlying header chain is invalid.
		InvalidAuthoritySet,
		/// There are too many requests for the current window to handle.
		TooManyRequests,
		/// The header being imported is older than the best finalized header known to the pallet.
		OldHeader,
		/// The header is unknown to the pallet.
		UnknownHeader,
		/// The scheduled authority set change found in the header is unsupported by the pallet.
		///
		/// This is the case for non-standard (e.g forced) authority set changes.
		UnsupportedScheduledChange,
		/// The pallet has already been initialized.
		AlreadyInitialized,
		/// All pallet operations are halted.
		Halted,
		/// The storage proof doesn't contains storage root. So it is invalid for given header.
		StorageRootMismatch,
	}

	/// Check the given header for a GRANDPA scheduled authority set change. If a change
	/// is found it will be enacted immediately.
	///
	/// This function does not support forced changes, or scheduled changes with delays
	/// since these types of changes are indicitive of abnormal behaviour from GRANDPA.
	pub(crate) fn try_enact_authority_change<T: Config<I>, I: 'static>(
		header: &BridgedHeader<T, I>,
	) -> Result<(), sp_runtime::DispatchError> {
		// We don't support forced changes - at that point governance intervention is required.
		ensure!(
			super::find_forced_change(header).is_none(),
			<Error<T, I>>::UnsupportedScheduledChange
		);

		if let Some(change) = super::find_scheduled_change(header) {
			// GRANDPA only includes a `delay` for forced changes, so this isn't valid.
			ensure!(change.delay == Zero::zero(), <Error<T, I>>::UnsupportedScheduledChange);

			let current_set_id = <CurrentAuthoritySet<T, I>>::get().set_id;
			// TODO [#788]: Stop manually increasing the `set_id` here.
			let next_authorities = bp_header_chain::AuthoritySet {
				authorities: change.next_authorities,
				set_id: current_set_id + 1,
			};

			// Since our header schedules a change and we know the delay is 0, it must also enact
			// the change.
			<CurrentAuthoritySet<T, I>>::put(&next_authorities);

			log::info!(
				target: "runtime::bridge-grandpa",
				"Transitioned from authority set {} to {}! New authorities are: {:?}",
				current_set_id,
				current_set_id + 1,
				next_authorities,
			);
		};

		Ok(())
	}

	/// Verify a GRANDPA justification (finality proof) for a given header.
	///
	/// Will use the GRANDPA current authorities known to the pallet.
	pub(crate) fn verify_justification<T: Config<I>, I: 'static>(
		justification: &[u8],
		hash: BridgedBlockHash<T, I>,
		number: BridgedBlockNumber<T, I>,
	) -> Result<(), sp_runtime::DispatchError> {
		use bp_header_chain::justification::verify_justification;

		let authority_set = <CurrentAuthoritySet<T, I>>::get();
		let voter_set = VoterSet::new(authority_set.authorities).ok_or(<Error<T, I>>::InvalidAuthoritySet)?;
		let set_id = authority_set.set_id;

		Ok(
			verify_justification::<BridgedHeader<T, I>>((hash, number), set_id, &voter_set, &justification).map_err(
				|e| {
					log::error!(target: "runtime::bridge-grandpa", "Received invalid justification for {:?}: {:?}", hash, e);
					<Error<T, I>>::InvalidJustification
				},
			)?,
		)
	}

	/// Since this writes to storage with no real checks this should only be used in functions that
	/// were called by a trusted origin.
	pub(crate) fn initialize_bridge<T: Config<I>, I: 'static>(
		init_params: super::InitializationData<BridgedHeader<T, I>>,
	) {
		let super::InitializationData {
			header,
			authority_list,
			set_id,
			is_halted,
		} = init_params;

		let initial_hash = header.hash();
		<InitialHash<T, I>>::put(initial_hash);
		<BestFinalized<T, I>>::put(initial_hash);
		<ImportedHeaders<T, I>>::insert(initial_hash, header);

		let authority_set = bp_header_chain::AuthoritySet::new(authority_list, set_id);
		<CurrentAuthoritySet<T, I>>::put(authority_set);

		<IsHalted<T, I>>::put(is_halted);
	}

	/// Ensure that the origin is either root, or `ModuleOwner`.
	fn ensure_owner_or_root<T: Config<I>, I: 'static>(origin: T::Origin) -> Result<(), BadOrigin> {
		match origin.into() {
			Ok(RawOrigin::Root) => Ok(()),
			Ok(RawOrigin::Signed(ref signer)) if Some(signer) == <ModuleOwner<T, I>>::get().as_ref() => Ok(()),
			_ => Err(BadOrigin),
		}
	}

	/// Ensure that the pallet is in operational mode (not halted).
	fn ensure_operational<T: Config<I>, I: 'static>() -> Result<(), Error<T, I>> {
		if <IsHalted<T, I>>::get() {
			Err(<Error<T, I>>::Halted)
		} else {
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Get the best finalized header the pallet knows of.
	///
	/// Returns a dummy header if there is no best header. This can only happen
	/// if the pallet has not been initialized yet.
	pub fn best_finalized() -> BridgedHeader<T, I> {
		let hash = <BestFinalized<T, I>>::get();
		<ImportedHeaders<T, I>>::get(hash).unwrap_or_else(|| {
			<BridgedHeader<T, I>>::new(
				Default::default(),
				Default::default(),
				Default::default(),
				Default::default(),
				Default::default(),
			)
		})
	}

	/// Check if a particular header is known to the bridge pallet.
	pub fn is_known_header(hash: BridgedBlockHash<T, I>) -> bool {
		<ImportedHeaders<T, I>>::contains_key(hash)
	}

	/// Verify that the passed storage proof is valid, given it is crafted using
	/// known finalized header. If the proof is valid, then the `parse` callback
	/// is called and the function returns its result.
	pub fn parse_finalized_storage_proof<R>(
		hash: BridgedBlockHash<T, I>,
		storage_proof: sp_trie::StorageProof,
		parse: impl FnOnce(bp_runtime::StorageProofChecker<BridgedBlockHasher<T, I>>) -> R,
	) -> Result<R, sp_runtime::DispatchError> {
		let header = <ImportedHeaders<T, I>>::get(hash).ok_or(Error::<T, I>::UnknownHeader)?;
		let storage_proof_checker = bp_runtime::StorageProofChecker::new(*header.state_root(), storage_proof)
			.map_err(|_| Error::<T, I>::StorageRootMismatch)?;

		Ok(parse(storage_proof_checker))
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

/// (Re)initialize bridge with given header for using it in external benchmarks.
#[cfg(feature = "runtime-benchmarks")]
pub fn initialize_for_benchmarks<T: Config<I>, I: 'static>(header: BridgedHeader<T, I>) {
	initialize_bridge::<T, I>(InitializationData {
		header,
		authority_list: Vec::new(), // we don't verify any proofs in external benchmarks
		set_id: 0,
		is_halted: false,
	});
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{run_test, test_header, Origin, TestHash, TestHeader, TestNumber, TestRuntime};
	use bp_test_utils::{
		authority_list, make_default_justification, make_justification_for_header, JustificationGeneratorParams, ALICE,
		BOB,
	};
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

	fn submit_finality_proof(header: u8) -> frame_support::dispatch::DispatchResultWithPostInfo {
		let header = test_header(header.into());
		let justification = make_default_justification(&header).encode();

		Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification)
	}

	fn next_block() {
		use frame_support::traits::OnInitialize;

		let current_number = frame_system::Module::<TestRuntime>::block_number();
		frame_system::Module::<TestRuntime>::set_block_number(current_number + 1);
		let _ = Module::<TestRuntime>::on_initialize(current_number);
	}

	fn change_log(delay: u64) -> Digest<TestHash> {
		let consensus_log = ConsensusLog::<TestNumber>::ScheduledChange(sp_finality_grandpa::ScheduledChange {
			next_authorities: vec![(ALICE.into(), 1), (BOB.into(), 1)],
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
				next_authorities: vec![(ALICE.into(), 1), (BOB.into(), 1)],
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
				BridgedBlockHash::<TestRuntime, ()>::default()
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
				Module::<TestRuntime>::set_operational(Origin::signed(2), false),
				DispatchError::BadOrigin,
			);
			assert_ok!(Module::<TestRuntime>::set_operational(Origin::root(), false));

			assert_ok!(Module::<TestRuntime>::set_owner(Origin::signed(1), None));
			assert_noop!(
				Module::<TestRuntime>::set_operational(Origin::signed(1), true),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				Module::<TestRuntime>::set_operational(Origin::signed(2), true),
				DispatchError::BadOrigin,
			);
			assert_ok!(Module::<TestRuntime>::set_operational(Origin::root(), true));
		});
	}

	#[test]
	fn pallet_may_be_halted_by_root() {
		run_test(|| {
			assert_ok!(Module::<TestRuntime>::set_operational(Origin::root(), false));
			assert_ok!(Module::<TestRuntime>::set_operational(Origin::root(), true));
		});
	}

	#[test]
	fn pallet_may_be_halted_by_owner() {
		run_test(|| {
			ModuleOwner::<TestRuntime>::put(2);

			assert_ok!(Module::<TestRuntime>::set_operational(Origin::signed(2), false));
			assert_ok!(Module::<TestRuntime>::set_operational(Origin::signed(2), true));

			assert_noop!(
				Module::<TestRuntime>::set_operational(Origin::signed(1), false),
				DispatchError::BadOrigin,
			);
			assert_noop!(
				Module::<TestRuntime>::set_operational(Origin::signed(1), true),
				DispatchError::BadOrigin,
			);

			assert_ok!(Module::<TestRuntime>::set_operational(Origin::signed(2), false));
			assert_noop!(
				Module::<TestRuntime>::set_operational(Origin::signed(1), true),
				DispatchError::BadOrigin,
			);
		});
	}

	#[test]
	fn pallet_rejects_transactions_if_halted() {
		run_test(|| {
			<IsHalted<TestRuntime>>::put(true);

			assert_noop!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), test_header(1), vec![]),
				Error::<TestRuntime>::Halted,
			);
		})
	}

	#[test]
	fn succesfully_imports_header_with_valid_finality() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1));

			let header = test_header(1);
			assert_eq!(<BestFinalized<TestRuntime>>::get(), header.hash());
			assert!(<ImportedHeaders<TestRuntime>>::contains_key(header.hash()));
		})
	}

	#[test]
	fn rejects_justification_that_skips_authority_set_transition() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(1);

			let params = JustificationGeneratorParams::<TestHeader> {
				set_id: 2,
				..Default::default()
			};
			let justification = make_justification_for_header(params).encode();

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification,),
				<Error<TestRuntime>>::InvalidJustification
			);
		})
	}

	#[test]
	fn does_not_import_header_with_invalid_finality_proof() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(1);
			let justification = [1u8; 32].encode();

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification,),
				<Error<TestRuntime>>::InvalidJustification
			);
		})
	}

	#[test]
	fn disallows_invalid_authority_set() {
		run_test(|| {
			let genesis = test_header(0);

			let invalid_authority_list = vec![(ALICE.into(), u64::MAX), (BOB.into(), u64::MAX)];
			let init_data = InitializationData {
				header: genesis,
				authority_list: invalid_authority_list,
				set_id: 1,
				is_halted: false,
			};

			assert_ok!(Module::<TestRuntime>::initialize(Origin::root(), init_data));

			let header = test_header(1);
			let justification = [1u8; 32].encode();

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification,),
				<Error<TestRuntime>>::InvalidAuthoritySet
			);
		})
	}

	#[test]
	fn importing_header_ensures_that_chain_is_extended() {
		run_test(|| {
			initialize_substrate_bridge();

			assert_ok!(submit_finality_proof(4));
			assert_err!(submit_finality_proof(3), Error::<TestRuntime>::OldHeader);
			assert_ok!(submit_finality_proof(5));
		})
	}

	#[test]
	fn importing_header_enacts_new_authority_set() {
		run_test(|| {
			initialize_substrate_bridge();

			let next_set_id = 2;
			let next_authorities = vec![(ALICE.into(), 1), (BOB.into(), 1)];

			// Need to update the header digest to indicate that our header signals an authority set
			// change. The change will be enacted when we import our header.
			let mut header = test_header(2);
			header.digest = change_log(0);

			// Create a valid justification for the header
			let justification = make_default_justification(&header).encode();

			// Let's import our test header
			assert_ok!(Module::<TestRuntime>::submit_finality_proof(
				Origin::signed(1),
				header.clone(),
				justification
			));

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

			// Create a valid justification for the header
			let justification = make_default_justification(&header).encode();

			// Should not be allowed to import this header
			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification),
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

			// Create a valid justification for the header
			let justification = make_default_justification(&header).encode();

			// Should not be allowed to import this header
			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification),
				<Error<TestRuntime>>::UnsupportedScheduledChange
			);
		})
	}

	#[test]
	fn parse_finalized_storage_proof_rejects_proof_on_unknown_header() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime>::parse_finalized_storage_proof(
					Default::default(),
					sp_trie::StorageProof::new(vec![]),
					|_| (),
				),
				Error::<TestRuntime>::UnknownHeader,
			);
		});
	}

	#[test]
	fn parse_finalized_storage_accepts_valid_proof() {
		run_test(|| {
			let (state_root, storage_proof) = bp_runtime::craft_valid_storage_proof();

			let mut header = test_header(2);
			header.set_state_root(state_root);

			let hash = header.hash();
			<BestFinalized<TestRuntime>>::put(hash);
			<ImportedHeaders<TestRuntime>>::insert(hash, header);

			assert_ok!(
				Module::<TestRuntime>::parse_finalized_storage_proof(hash, storage_proof, |_| (),),
				(),
			);
		});
	}

	#[test]
	fn rate_limiter_disallows_imports_once_limit_is_hit_in_single_block() {
		run_test(|| {
			initialize_substrate_bridge();

			assert_ok!(submit_finality_proof(1));
			assert_ok!(submit_finality_proof(2));
			assert_err!(submit_finality_proof(3), <Error<TestRuntime>>::TooManyRequests);
		})
	}

	#[test]
	fn rate_limiter_invalid_requests_do_not_count_towards_request_count() {
		run_test(|| {
			let submit_invalid_request = || {
				let header = test_header(1);
				let invalid_justification = vec![4, 2, 4, 2].encode();

				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, invalid_justification)
			};

			initialize_substrate_bridge();

			for _ in 0..<TestRuntime as Config>::MaxRequests::get() + 1 {
				// Notice that the error here *isn't* `TooManyRequests`
				assert_err!(submit_invalid_request(), <Error<TestRuntime>>::InvalidJustification);
			}

			// Can still submit `MaxRequests` requests afterwards
			assert_ok!(submit_finality_proof(1));
			assert_ok!(submit_finality_proof(2));
			assert_err!(submit_finality_proof(3), <Error<TestRuntime>>::TooManyRequests);
		})
	}

	#[test]
	fn rate_limiter_allows_request_after_new_block_has_started() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1));
			assert_ok!(submit_finality_proof(2));

			next_block();
			assert_ok!(submit_finality_proof(3));
		})
	}

	#[test]
	fn rate_limiter_disallows_imports_once_limit_is_hit_across_different_blocks() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1));
			assert_ok!(submit_finality_proof(2));

			next_block();
			assert_ok!(submit_finality_proof(3));
			assert_err!(submit_finality_proof(4), <Error<TestRuntime>>::TooManyRequests);
		})
	}

	#[test]
	fn rate_limiter_allows_max_requests_after_long_time_with_no_activity() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1));
			assert_ok!(submit_finality_proof(2));

			next_block();
			next_block();

			next_block();
			assert_ok!(submit_finality_proof(5));
			assert_ok!(submit_finality_proof(7));
		})
	}
}
