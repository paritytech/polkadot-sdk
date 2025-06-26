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

//! # People Pallet
//!
//! A pallet managing the registry of proven individuals.
//!
//! ## Overview
//!
//! The People pallet stores and manages identifiers of individuals who have proven their
//! personhood. It tracks their personal IDs, organizes their cryptographic keys into rings, and
//! allows them to use contextual aliases through authentication in extensions. When transactions
//! include cryptographic proofs of belonging to the people set, the pallet's transaction extension
//! verifies these proofs before allowing the transaction to proceed. This enables other pallets to
//! check if actions come from unique persons while preserving privacy through the ring-based
//! structure.
//!
//! The pallet accepts new persons after they prove their uniqueness elsewhere, stores their
//! information, and supports removing persons via suspensions. While other systems (e.g., wallets)
//! generate the proofs, this pallet handles the storage of all necessary data and verifies the
//! proofs when used.
//!
//! ## Key Features
//!
//! - **Stores Identity Data**: Tracks personal IDs and cryptographic keys of proven persons
//! - **Organizes Keys**: Groups keys into rings to enable privacy-preserving proofs
//! - **Verifies Proofs**: Checks personhood proofs attached to transactions
//! - **Links Accounts**: Allows connecting blockchain accounts to contextual aliases
//! - **Manages Registry**: Adds proven persons and will support removing them
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `set_alias_account(origin, account)`: Link an account to a contextual alias Once linked, this
//!   allows the account to dispatch transactions as a person with the alias origin using a regular
//!   signed transaction with a nonce, providing a simpler alternative to attaching full proofs.
//! - `unset_alias_account(origin)`: Remove an account-alias link.
//! - `merge_rings`: Merge the people in two rings into a single, new ring.
//! - `force_recognize_personhood`: Recognize a set of people without any additional checks.
//! - `set_personal_id_account`: Set a personal id account.
//! - `unset_personal_id_account`: Unset the personal id account.
//! - `migrate_included_key`: Migrate the key for a person who was onboarded and is currently
//!   included in a ring.
//! - `migrate_onboarding_key`: Migrate the key for a person who is currently onboarding. The
//!   operation is instant, replacing the old key in the onboarding queue.
//! - `set_onboarding_size`: Force set the onboarding size for new people. This call requires root
//!   privileges.
//! - `build_ring_manual`: Manually build a ring root by including registered people. The
//!   transaction fee is refunded on a successful call.
//! - `onboard_people_manual`: Manually onboard people into a ring. The transaction fee is refunded
//!   on a successful call.
//!
//! ### Automated tasks performed by the pallet in hooks
//!
//! - Ring building: Build or update a ring's cryptographic commitment. This task processes queued
//!   keys into a ring commitment that enables proof generation and verification. Since ring
//!   construction, or rather adding keys to the ring, is computationally expensive, it's performed
//!   periodically in batches rather than processing each key immediately. The batch size needs to
//!   be reasonably large to enhance privacy by obscuring the exact timing of when individuals' keys
//!   were added to the ring, making it more difficult to correlate specific persons with their
//!   keys.
//! - People onboarding: Onboard people from the onboarding queue into a ring. This task takes the
//!   unincluded keys of recognized people from the onboarding queue and registers them into the
//!   ring. People can be onboarded only in batches of at least `OnboardingSize` and when the
//!   remaining open slots in a ring are at least `OnboardingSize`. This does not compute the root,
//!   that is done using `build_ring`.
//! - Cleaning of suspended people: Remove people's keys marked as suspended or inactive from rings.
//!   The keys are stored in the `PendingSuspensions` map and they are removed from rings and their
//!   roots are reset. The ring roots will subsequently be build in the ring building phase from
//!   scratch. sequentially.
//! - Key migration: Migrate the keys for people who were onboarded and are currently included in
//!   rings. The migration is not instant as the key replacement and subsequent inclusion in a new
//!   ring root will happen only after the next mutation session.
//! - Onboarding queue page merging: Merge the two pages at the front of the onboarding queue. After
//!   a round of suspensions, it is possible for the second page of the onboarding queue to be left
//!   with few members such that, if the first page also has few members, the total count is below
//!   the required onboarding size, thus stalling the queue. This function fixes this by moving the
//!   people from the first page to the front of the second page, defragmenting the queue.
//!
//! ### Transaction Extension
//!
//! The pallet provides the `AsPerson` transaction extension that allows transactions to be
//! dispatched with special origins: `PersonalIdentity` and `PersonalAlias`. These origins prove the
//! transaction comes from a unique person, either through their identity or through a contextual
//! alias. To make use of the personhood system, other pallets should check for these origins.
//!
//! The extension verifies the proof of personhood during transaction validation and, if valid,
//! transforms the transaction's origin into one of these special origins.
//!
//! ## Usage
//!
//! Other pallets can verify personhood through origin checks:
//!
//! - `EnsurePersonalIdentity`: Verifies the origin represents a specific person using their
//!   PersonalId
//! - `EnsurePersonalAlias`: Verifies the origin has a valid alias for any context
//! - `EnsurePersonalAliasInContext`: Verifies the origin has a valid alias for a specific context

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "128"]
#![allow(clippy::borrowed_box)]
extern crate alloc;
use alloc::{boxed::Box, vec::Vec};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod extension;
pub mod types;
pub mod weights;
pub use pallet::*;
pub use types::*;
pub use weights::WeightInfo;

use codec::{Decode, Encode, MaxEncodedLen};
use core::{
	cmp::{self},
	ops::Range,
};
use frame_support::{
	dispatch::{
		extract_actual_weight, DispatchInfo, DispatchResultWithPostInfo, GetDispatchInfo,
		PostDispatchInfo,
	},
	storage::with_storage_layer,
	traits::{
		reality::{
			AddOnlyPeopleTrait, Context, ContextualAlias, CountedMembers, PeopleTrait, PersonalId,
			RingIndex,
		},
		Defensive, EnsureOriginWithArg, IsSubType, OriginTrait,
	},
	transactional,
	weights::WeightMeter,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{BadOrigin, Dispatchable},
	ArithmeticError, RuntimeDebug, SaturatedConversion, Saturating,
};
use verifiable::{Alias, GenerateVerifiable};

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::BenchmarkHelper;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, traits::Contains};
	use frame_system::pallet_prelude::{BlockNumberFor, *};

	const LOG_TARGET: &str = "runtime::people";

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config<
		RuntimeOrigin: From<Origin>
		                   + From<<Self::RuntimeOrigin as OriginTrait>::PalletsOrigin>
		                   + OriginTrait<
			PalletsOrigin: From<Origin>
			                   + TryInto<
				Origin,
				Error = <Self::RuntimeOrigin as OriginTrait>::PalletsOrigin,
			>,
		>,
		RuntimeCall: Parameter
		                 + GetDispatchInfo
		                 + IsSubType<Call<Self>>
		                 + Dispatchable<
			RuntimeOrigin = Self::RuntimeOrigin,
			Info = DispatchInfo,
			PostInfo = PostDispatchInfo,
		>,
	>
	{
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The runtime event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Trait allowing cryptographic proof of membership without exposing the underlying member.
		/// Normally a Ring-VRF.
		type Crypto: GenerateVerifiable<
			Proof: Send + Sync + DecodeWithMemTracking,
			Signature: Send + Sync + DecodeWithMemTracking,
			Member: DecodeWithMemTracking,
		>;

		/// Contexts which may validly have an account alias behind it for everyone.
		type AccountContexts: Contains<Context>;

		/// Number of chunks per page.
		#[pallet::constant]
		type ChunkPageSize: Get<u32>;

		/// Maximum number of people included in a ring before a new one is created.
		#[pallet::constant]
		type MaxRingSize: Get<u32>;

		/// Maximum number of people included in an onboarding queue page before a new one is
		/// created.
		#[pallet::constant]
		type OnboardingQueuePageSize: Get<u32>;

		/// Helper for benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper<<Self::Crypto as GenerateVerifiable>::StaticChunk>;
	}

	/// The current individuals we recognise.
	#[pallet::storage]
	pub type Root<T> = StorageMap<_, Blake2_128Concat, RingIndex, RingRoot<T>>;

	/// Keeps track of the ring index currently being populated.
	#[pallet::storage]
	pub type CurrentRingIndex<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Maximum number of people queued before onboarding to a ring.
	#[pallet::storage]
	pub type OnboardingSize<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Hint for the maximum number of people that can be included in a ring through a single root
	/// building call. If no value is set, then the onboarding size will be used instead.
	#[pallet::storage]
	pub type RingBuildingPeopleLimit<T: Config> = StorageValue<_, u32, OptionQuery>;

	/// Both the keys that are included in built rings
	/// and the keys that will be used in future rings.
	#[pallet::storage]
	pub type RingKeys<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		RingIndex,
		BoundedVec<MemberOf<T>, T::MaxRingSize>,
		ValueQuery,
	>;

	/// Stores the meta information for each ring, the number of keys and how many are actually
	/// included in the root.
	#[pallet::storage]
	pub type RingKeysStatus<T: Config> =
		StorageMap<_, Blake2_128Concat, RingIndex, RingStatus, ValueQuery>;

	/// A map of all rings which currently have pending suspensions and need cleaning, along with
	/// their respective number of suspended keys which need to be removed.
	#[pallet::storage]
	pub type PendingSuspensions<T: Config> =
		StorageMap<_, Twox64Concat, RingIndex, BoundedVec<u32, T::MaxRingSize>, ValueQuery>;

	/// The number of people currently included in a ring.
	#[pallet::storage]
	pub type ActiveMembers<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The current individuals we recognise, but not necessarily yet included in a ring.
	///
	/// Look-up from the crypto (public) key to the immutable ID of the individual (`PersonalId`). A
	/// person can have two different entries in this map if they queued a key migration which
	/// hasn't been enacted yet.
	#[pallet::storage]
	pub type Keys<T> = CountedStorageMap<_, Blake2_128Concat, MemberOf<T>, PersonalId>;

	/// A map of all the people who have declared their intent to migrate their keys and are waiting
	/// for the next mutation session.
	#[pallet::storage]
	pub type KeyMigrationQueue<T: Config> =
		StorageMap<_, Blake2_128Concat, PersonalId, MemberOf<T>>;

	/// The current individuals we recognise, but not necessarily yet included in a ring.
	///
	/// Immutable ID of the individual (`PersonalId`) to information about their key and status.
	#[pallet::storage]
	pub type People<T: Config> =
		StorageMap<_, Blake2_128Concat, PersonalId, PersonRecord<MemberOf<T>, T::AccountId>>;

	/// Conversion of a contextual alias to an account ID.
	#[pallet::storage]
	pub type AliasToAccount<T> = StorageMap<
		_,
		Blake2_128Concat,
		ContextualAlias,
		<T as frame_system::Config>::AccountId,
		OptionQuery,
	>;

	/// Conversion of an account ID to a contextual alias.
	#[pallet::storage]
	pub type AccountToAlias<T> = StorageMap<
		_,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		RevisedContextualAlias,
		OptionQuery,
	>;

	/// Association of an account ID to a personal ID.
	///
	/// Managed with `set_personal_id_account` and `unset_personal_id_account`.
	/// Reverse lookup is inside `People` storage, inside the record.
	#[pallet::storage]
	pub type AccountToPersonalId<T> = StorageMap<
		_,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		PersonalId,
		OptionQuery,
	>;

	/// Paginated collection of static chunks used by the verifiable crypto.
	#[pallet::storage]
	pub type Chunks<T> = StorageMap<_, Twox64Concat, PageIndex, ChunksOf<T>, OptionQuery>;

	/// The next free and never reserved personal ID.
	#[pallet::storage]
	pub type NextPersonalId<T> = StorageValue<_, PersonalId, ValueQuery>;

	/// The state of the pallet regarding the actions that are currently allowed to be performed on
	/// all existing rings.
	#[pallet::storage]
	pub type RingsState<T> = StorageValue<_, RingMembersState, ValueQuery>;

	/// Candidates' reserved identities which we track.
	#[pallet::storage]
	pub type ReservedPersonalId<T: Config> =
		StorageMap<_, Twox64Concat, PersonalId, (), OptionQuery>;

	/// Keeps track of the page indices of the head and tail of the onboarding queue.
	#[pallet::storage]
	pub type QueuePageIndices<T: Config> = StorageValue<_, (PageIndex, PageIndex), ValueQuery>;

	/// Paginated collection of people public keys ready to be included in a ring.
	#[pallet::storage]
	pub type OnboardingQueue<T> = StorageMap<
		_,
		Twox64Concat,
		PageIndex,
		BoundedVec<MemberOf<T>, <T as Config>::OnboardingQueuePageSize>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An individual has had their personhood recognised and indexed.
		PersonhoodRecognized { who: PersonalId, key: MemberOf<T> },
		/// An individual has had their personhood recognised again and indexed.
		PersonOnboarding { who: PersonalId, key: MemberOf<T> },
	}

	#[pallet::extra_constants]
	impl<T: Config> Pallet<T> {
		/// The amount of block number tolerance we allow for a setup account transaction.
		///
		/// `set_alias_account` and `set_personal_id_account` calls contains
		/// `call_valid_at` as a parameter, those calls are valid if the block number is within
		/// the tolerance period.
		pub fn account_setup_time_tolerance() -> BlockNumberFor<T> {
			600u32.into()
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The supplied identifier does not represent a person.
		NotPerson,
		/// The given person has no associated key.
		NoKey,
		/// The context is not a member of those allowed to have account aliases held.
		InvalidContext,
		/// The account is not known.
		InvalidAccount,
		/// The account is already in use under another alias.
		AccountInUse,
		/// The proof is invalid.
		InvalidProof,
		/// The signature is invalid.
		InvalidSignature,
		/// There are not yet any members of our personhood set.
		NoMembers,
		/// The root cannot be finalized as there are still unpushed members.
		Incomplete,
		/// The root is still fresh.
		StillFresh,
		/// Too many members have been pushed.
		TooManyMembers,
		/// Key already in use by another person.
		KeyAlreadyInUse,
		/// The old key was not found when expected.
		KeyNotFound,
		/// Could not push member into the ring.
		CouldNotPush,
		/// The record is already using this key.
		SameKey,
		/// Personal Id was not reserved.
		PersonalIdNotReserved,
		/// Personal Id has never been reserved.
		PersonalIdReservationCannotRenew,
		/// Personal Id was not reserved or not already recognized.
		PersonalIdNotReservedOrNotRecognized,
		/// Ring cannot be merged if it's the top ring.
		InvalidRing,
		/// Ring cannot be built while there are suspensions pending.
		SuspensionsPending,
		/// Ring cannot be merged if it's not below 1/2 capacity.
		RingAboveMergeThreshold,
		/// Suspension indices provided are invalid.
		InvalidSuspensions,
		/// An mutating action was queued when there was no mutation session in progress.
		NoMutationSession,
		/// An mutating session could not be started.
		CouldNotStartMutationSession,
		/// Cannot merge rings while a suspension session is in progress.
		SuspensionSessionInProgress,
		/// Call is too late or too early.
		TimeOutOfRange,
		/// Alias <-> Account is already set and up to date.
		AliasAccountAlreadySet,
		/// Personhood cannot be resumed if it is not suspended.
		NotSuspended,
		/// Personhood is suspended.
		Suspended,
		/// Invalid state for attempted key migration.
		InvalidKeyMigration,
		/// Invalid suspension of a key belonging to a person whose index in the ring has already
		/// been included in the pending suspensions list.
		KeyAlreadySuspended,
		/// The onboarding size must not exceed the maximum ring size.
		InvalidOnboardingSize,
	}

	#[pallet::origin]
	#[derive(
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		Encode,
		Decode,
		MaxEncodedLen,
		TypeInfo,
		DecodeWithMemTracking,
	)]
	pub enum Origin {
		PersonalIdentity(PersonalId),
		PersonalAlias(RevisedContextualAlias),
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(
				<T as Config>::ChunkPageSize::get() > 0,
				"chunk page size must hold at least one element"
			);
			assert!(<T as Config>::MaxRingSize::get() > 0, "rings must hold at least one person");
			assert!(
				<T as Config>::MaxRingSize::get() <= <T as Config>::OnboardingQueuePageSize::get(),
				"onboarding queue page size must greater than or equal to max ring size"
			);
		}

		fn on_poll(_: BlockNumberFor<T>, weight_meter: &mut WeightMeter) {
			// Check if there are any keys to migrate.
			if weight_meter.try_consume(T::WeightInfo::on_poll_base()).is_err() {
				return;
			}
			if RingsState::<T>::get().key_migration() {
				Self::migrate_keys(weight_meter);
			}

			// Check if there are any rings with suspensions and try to clean the first one.
			if let Some(ring_index) = PendingSuspensions::<T>::iter_keys().next() {
				if Self::should_remove_suspended_keys(ring_index, true) &&
					weight_meter.can_consume(T::WeightInfo::remove_suspended_people(
						T::MaxRingSize::get(),
					)) {
					let actual = Self::remove_suspended_keys(ring_index);
					weight_meter.consume(actual)
				}
			}

			let merge_weight = T::WeightInfo::merge_queue_pages();
			if !weight_meter.can_consume(merge_weight) {
				return;
			}
			let merge_action = Self::should_merge_queue_pages();
			if let QueueMergeAction::Merge {
				initial_head,
				new_head,
				first_key_page,
				second_key_page,
			} = merge_action
			{
				Self::merge_queue_pages(initial_head, new_head, first_key_page, second_key_page);
				weight_meter.consume(merge_weight);
			}
		}

		fn on_idle(_block: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut weight_meter = WeightMeter::with_limit(limit.saturating_div(2));
			let on_idle_weight = T::WeightInfo::on_idle_base();
			if !weight_meter.can_consume(on_idle_weight) {
				return weight_meter.consumed();
			}
			weight_meter.consume(on_idle_weight);

			let max_ring_size = T::MaxRingSize::get();
			let remove_people_weight = T::WeightInfo::remove_suspended_people(max_ring_size);
			let rings_state = RingsState::<T>::get();

			// Check if there are any rings with suspensions and try to clean as many as possible.
			// First check the state of the rings allow for removals.
			if !rings_state.append_only() {
				return weight_meter.consumed();
			}
			// Account for the first iteration of the loop.
			let suspension_step_weight = T::WeightInfo::pending_suspensions_iteration();
			if !weight_meter.can_consume(suspension_step_weight) {
				return weight_meter.consumed();
			}
			// Always renew the iterator because in each iteration we remove a key, which would make
			// the old iterator unstable.
			while let Some(ring_index) = PendingSuspensions::<T>::iter_keys().next() {
				weight_meter.consume(suspension_step_weight);
				// Break the loop if we run out of weight.
				if !weight_meter.can_consume(remove_people_weight) {
					return weight_meter.consumed();
				}
				if Self::should_remove_suspended_keys(ring_index, false) {
					let actual = Self::remove_suspended_keys(ring_index);
					weight_meter.consume(actual)
				}
				// Break the loop if we run out of weight.
				if !weight_meter.can_consume(suspension_step_weight) {
					return weight_meter.consumed();
				}
			}

			// Ring state must be append only for both onboarding and ring building, but it is
			// already checked above.

			let onboard_people_weight = T::WeightInfo::onboard_people();
			if !weight_meter.can_consume(onboard_people_weight) {
				return weight_meter.consumed();
			}
			let op_res = with_storage_layer::<(), DispatchError, _>(|| Self::onboard_people());
			weight_meter.consume(onboard_people_weight);
			if let Err(e) = op_res {
				log::debug!(target: LOG_TARGET, "failed to onboard people: {:?}", e);
			}

			let current_ring = CurrentRingIndex::<T>::get();
			let should_build_ring_weight = T::WeightInfo::should_build_ring(max_ring_size);
			let build_ring_weight = T::WeightInfo::build_ring(max_ring_size);
			for ring_index in (0..=current_ring).rev() {
				if !weight_meter.can_consume(should_build_ring_weight) {
					return weight_meter.consumed();
				}

				let maybe_to_include = Self::should_build_ring(ring_index, max_ring_size);
				weight_meter.consume(should_build_ring_weight);
				let Some(to_include) = maybe_to_include else { continue };
				if !weight_meter.can_consume(build_ring_weight) {
					return weight_meter.consumed();
				}
				let op_res = with_storage_layer::<(), DispatchError, _>(|| {
					Self::build_ring(ring_index, to_include)
				});
				weight_meter.consume(build_ring_weight);
				if let Err(e) = op_res {
					log::error!(target: LOG_TARGET, "failed to build ring: {:?}", e);
				}
			}

			weight_meter.consumed()
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub encoded_chunks: Vec<u8>,
		#[serde(skip)]
		pub _phantom_data: core::marker::PhantomData<T>,
		pub onboarding_size: u32,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			// The default genesis config will put in the chunks that pertain to the ring vrf
			// implementation in the `verifiable` crate. This default config will not work for other
			// custom `GenerateVerifiable` implementations.
			use verifiable::ring_vrf_impl::StaticChunk;
			let params = verifiable::ring_vrf_impl::ring_verifier_builder_params();
			let chunks: Vec<StaticChunk> = params.0.iter().map(|c| StaticChunk(*c)).collect();
			Self {
				encoded_chunks: chunks.encode(),
				_phantom_data: PhantomData,
				onboarding_size: T::MaxRingSize::get(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let chunks: Vec<<<T as Config>::Crypto as GenerateVerifiable>::StaticChunk> =
				Decode::decode(&mut &(self.encoded_chunks.clone())[..])
					.expect("couldn't decode chunks");
			assert_eq!(chunks.len(), 1 << 9);
			let page_size = <T as Config>::ChunkPageSize::get();

			let mut page_idx = 0;
			let mut chunk_idx = 0;
			while chunk_idx < chunks.len() {
				let chunk_idx_end = cmp::min(chunk_idx + page_size as usize, chunks.len());
				let chunk_page: ChunksOf<T> = chunks[chunk_idx..chunk_idx_end]
					.to_vec()
					.try_into()
					.expect("page size was checked against the array length; qed");
				Chunks::<T>::insert(page_idx, chunk_page);
				page_idx += 1;
				chunk_idx = chunk_idx_end;
			}

			OnboardingSize::<T>::set(self.onboarding_size);
		}
	}

	#[pallet::call(weight = <T as Config>::WeightInfo)]
	impl<T: Config> Pallet<T> {
		/// Build a ring root by including registered people.
		///
		/// This task is performed automatically by the pallet through the `on_idle` hook whenever
		/// there is leftover weight in a block. This call is meant to be a backup in case of
		/// extreme congestion and should be submitted by signed origins.
		#[pallet::weight(
			T::WeightInfo::should_build_ring(
				limit.unwrap_or_else(T::MaxRingSize::get)
			).saturating_add(T::WeightInfo::build_ring(limit.unwrap_or_else(T::MaxRingSize::get))))]
		#[pallet::call_index(100)]
		pub fn build_ring_manual(
			origin: OriginFor<T>,
			ring_index: RingIndex,
			limit: Option<u32>,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;

			// Get the keys for this ring, and make sure that the ring is full before we build it.
			let (keys, mut ring_status) = Self::ring_keys_and_info(ring_index);
			let to_include =
				Self::should_build_ring(ring_index, limit.unwrap_or_else(T::MaxRingSize::get))
					.ok_or(Error::<T>::StillFresh)?;

			// Get the current ring, and check it should be rebuilt.
			// Return the next revision.
			let (next_revision, mut intermediate) =
				if let Some(existing_root) = Root::<T>::get(ring_index) {
					// We should build a new ring. Return the new revision number we should use.
					(
						existing_root.revision.checked_add(1).ok_or(ArithmeticError::Overflow)?,
						existing_root.intermediate,
					)
				} else {
					// No ring has been built at this index, so we start at revision 0.
					(0, T::Crypto::start_members())
				};

			// Push the members.
			T::Crypto::push_members(
				&mut intermediate,
				keys.iter()
					.skip(ring_status.included as usize)
					.take(to_include as usize)
					.cloned(),
				Self::fetch_chunks,
			)
			.map_err(|_| Error::<T>::CouldNotPush)?;

			// By the end of the loop, we have included the maximum number of keys in the vector.
			ring_status.included = ring_status.included.saturating_add(to_include);
			RingKeysStatus::<T>::insert(ring_index, ring_status);

			// We create the root after pushing all members.
			let root = T::Crypto::finish_members(intermediate.clone());
			let ring_root = RingRoot { root, revision: next_revision, intermediate };
			Root::<T>::insert(ring_index, ring_root);

			Ok(Pays::No.into())
		}

		/// Onboard people into a ring by taking their keys from the onboarding queue and
		/// registering them into the ring. This does not compute the root, that is done using
		/// `build_ring`.
		///
		/// This task is performed automatically by the pallet through the `on_idle` hook whenever
		/// there is leftover weight in a block. This call is meant to be a backup in case of
		/// extreme congestion and should be submitted by signed origins.
		#[pallet::weight(T::WeightInfo::onboard_people())]
		#[pallet::call_index(101)]
		pub fn onboard_people_manual(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;

			// Get the keys for this ring, and make sure that the ring is full before we build it.
			let (top_ring_index, mut keys) = Self::available_ring();
			let mut ring_status = RingKeysStatus::<T>::get(top_ring_index);
			defensive_assert!(
				keys.len() == ring_status.total as usize,
				"Stored key count doesn't match the actual length"
			);

			let keys_len = keys.len() as u32;
			let open_slots = T::MaxRingSize::get().saturating_sub(keys_len);

			let (mut head, tail) = QueuePageIndices::<T>::get();
			let old_head = head;
			let mut keys_to_include: Vec<MemberOf<T>> =
				OnboardingQueue::<T>::take(head).into_inner();

			// A `head != tail` condition should mean that there is at least one key in the page
			// following this one.
			if keys_to_include.len() < open_slots as usize && head != tail {
				head = head.checked_add(1).unwrap_or(0);
				let second_key_page = OnboardingQueue::<T>::take(head);
				defensive_assert!(!second_key_page.is_empty());
				keys_to_include.extend(second_key_page.into_iter());
			}

			let onboarding_size = OnboardingSize::<T>::get();

			let (to_include, ring_filled) = Self::should_onboard_people(
				top_ring_index,
				&ring_status,
				open_slots,
				keys_to_include.len().saturated_into(),
				onboarding_size,
			)
			.ok_or(Error::<T>::Incomplete)?;

			let mut remaining_keys = keys_to_include.split_off(to_include as usize);
			for key in keys_to_include.into_iter() {
				let personal_id = Keys::<T>::get(&key).defensive().ok_or(Error::<T>::NotPerson)?;
				let mut record =
					People::<T>::get(personal_id).defensive().ok_or(Error::<T>::KeyNotFound)?;
				record.position = RingPosition::Included {
					ring_index: top_ring_index,
					ring_position: keys.len().saturated_into(),
					scheduled_for_removal: false,
				};
				People::<T>::insert(personal_id, record);
				keys.try_push(key).map_err(|_| Error::<T>::TooManyMembers)?;
			}
			RingKeys::<T>::insert(top_ring_index, keys);
			ActiveMembers::<T>::mutate(|active| *active = active.saturating_add(to_include));
			ring_status.total = ring_status.total.saturating_add(to_include);
			RingKeysStatus::<T>::insert(top_ring_index, ring_status);

			// Update the top ring index if this onboarding round filled the current ring.
			if ring_filled {
				CurrentRingIndex::<T>::mutate(|i| i.saturating_inc());
			}

			if remaining_keys.len() > T::OnboardingQueuePageSize::get() as usize {
				let split_idx =
					remaining_keys.len().saturating_sub(T::OnboardingQueuePageSize::get() as usize);
				let second_page_keys: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize> =
					remaining_keys
						.split_off(split_idx)
						.try_into()
						.expect("the list shrunk so it must fit; qed");
				let remaining_keys: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize> =
					remaining_keys.try_into().expect("the list shrunk so it must fit; qed");
				OnboardingQueue::<T>::insert(old_head, remaining_keys);
				OnboardingQueue::<T>::insert(head, second_page_keys);
				QueuePageIndices::<T>::put((old_head, tail));
			} else if !remaining_keys.is_empty() {
				let remaining_keys: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize> =
					remaining_keys.try_into().expect("the list shrunk so it must fit; qed");
				OnboardingQueue::<T>::insert(head, remaining_keys);
				QueuePageIndices::<T>::put((head, tail));
			} else {
				// We have nothing to put back into the queue, so if this isn't the last page, move
				// the head to the next page of the queue.
				if head != tail {
					head = head.checked_add(1).unwrap_or(0);
				}
				QueuePageIndices::<T>::put((head, tail));
			}

			Ok(Pays::No.into())
		}

		/// Merge the people in two rings into a single, new ring. In order for the rings to be
		/// eligible for merging, they must be below 1/2 of max capacity, have no pending
		/// suspensions and not be the top ring used for onboarding.
		#[pallet::call_index(102)]
		pub fn merge_rings(
			origin: OriginFor<T>,
			base_ring_index: RingIndex,
			target_ring_index: RingIndex,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;

			ensure!(RingsState::<T>::get().append_only(), Error::<T>::SuspensionSessionInProgress);
			// Top ring that onboards new candidates cannot be merged. Identical rings cannot be
			// merged.
			let current_ring_index = CurrentRingIndex::<T>::get();
			ensure!(
				base_ring_index != target_ring_index &&
					base_ring_index != current_ring_index &&
					target_ring_index != current_ring_index,
				Error::<T>::InvalidRing
			);

			// Enforce eligibility criteria.
			let (mut base_keys, mut base_ring_status) = Self::ring_keys_and_info(base_ring_index);
			ensure!(
				base_keys.len() < T::MaxRingSize::get() as usize / 2,
				Error::<T>::RingAboveMergeThreshold
			);
			ensure!(
				PendingSuspensions::<T>::decode_len(base_ring_index).unwrap_or(0) == 0,
				Error::<T>::SuspensionsPending
			);
			let target_keys = RingKeys::<T>::get(target_ring_index);
			RingKeysStatus::<T>::remove(target_ring_index);
			ensure!(
				target_keys.len() < T::MaxRingSize::get() as usize / 2,
				Error::<T>::RingAboveMergeThreshold
			);
			ensure!(
				PendingSuspensions::<T>::decode_len(target_ring_index).unwrap_or(0) == 0,
				Error::<T>::SuspensionsPending
			);

			// Update the status of the ring to reflect the newly added keys.
			base_ring_status.total =
				base_ring_status.total.saturating_add(target_keys.len().saturated_into());

			for key in target_keys {
				let personal_id =
					Keys::<T>::get(&key).defensive().ok_or(Error::<T>::KeyNotFound)?;
				let mut record =
					People::<T>::get(personal_id).defensive().ok_or(Error::<T>::NotPerson)?;
				record.position = RingPosition::Included {
					ring_index: base_ring_index,
					ring_position: base_keys.len().saturated_into(),
					scheduled_for_removal: false,
				};
				base_keys.try_push(key).map_err(|_| Error::<T>::TooManyMembers)?;
				People::<T>::insert(personal_id, record)
			}

			// Newly added keys are not yet included.
			RingKeys::<T>::insert(base_ring_index, base_keys);
			RingKeysStatus::<T>::insert(base_ring_index, base_ring_status);
			// Remove the stale ring root of the target ring. The keys in the target ring will be
			// part of a valid ring root again when the base ring is rebuilt.
			Root::<T>::remove(target_ring_index);
			RingKeys::<T>::remove(target_ring_index);
			RingKeysStatus::<T>::remove(target_ring_index);

			Ok(Pays::No.into())
		}

		/// Dispatch a call under an alias using the `account <-> alias` mapping.
		///
		/// This is a call version of the transaction extension `AsPersonalAliasWithAccount`.
		/// It is recommended to use the transaction extension instead when suitable.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::under_alias().saturating_add(call.get_dispatch_info().call_weight))]
		pub fn under_alias(
			origin: OriginFor<T>,
			call: Box<<T as frame_system::Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			let account = ensure_signed(origin.clone())?;
			let rev_ca = AccountToAlias::<T>::get(&account).ok_or(Error::<T>::InvalidAccount)?;
			ensure!(
				Root::<T>::get(rev_ca.ring).is_some_and(|ring| ring.revision == rev_ca.revision),
				DispatchError::BadOrigin,
			);

			let derivation_weight = T::WeightInfo::under_alias();
			let local_origin = Origin::PersonalAlias(rev_ca);
			Self::derivative_call(origin, local_origin, *call, derivation_weight)
		}

		/// This transaction is refunded if successful and no alias was previously set.
		///
		/// The call is valid from `call_valid_at` until
		/// `call_valid_at + account_setup_time_tolerance`.
		/// `account_setup_time_tolerance` is a constant available in the metadata.
		///
		/// Parameters:
		/// - `account`: The account to set the alias for.
		/// - `call_valid_at`: The block number when the call becomes valid.
		#[pallet::call_index(1)]
		pub fn set_alias_account(
			origin: OriginFor<T>,
			account: T::AccountId,
			call_valid_at: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			let rev_ca = Self::ensure_revised_personal_alias(origin)?;
			let now = frame_system::Pallet::<T>::block_number();
			let time_tolerance = Self::account_setup_time_tolerance();
			ensure!(
				call_valid_at <= now && now <= call_valid_at.saturating_add(time_tolerance),
				Error::<T>::TimeOutOfRange
			);
			ensure!(T::AccountContexts::contains(&rev_ca.ca.context), Error::<T>::InvalidContext);
			ensure!(!AccountToPersonalId::<T>::contains_key(&account), Error::<T>::AccountInUse);

			let old_account = AliasToAccount::<T>::get(&rev_ca.ca);
			let old_rev_ca = old_account.as_ref().and_then(AccountToAlias::<T>::get);

			let needs_revision = old_rev_ca.is_some_and(|old_rev_ca| {
				old_rev_ca.revision != rev_ca.revision || old_rev_ca.ring != rev_ca.ring
			});

			// Ensure it changes the account associated, or it needs revision.
			ensure!(
				old_account.as_ref() != Some(&account) || needs_revision,
				Error::<T>::AliasAccountAlreadySet
			);

			// If the old account is different from the new one:
			// * decrease the sufficients of the old account
			// * increase the sufficients of the new account
			// * check new account is not already in use
			if old_account.as_ref() != Some(&account) {
				ensure!(!AccountToAlias::<T>::contains_key(&account), Error::<T>::AccountInUse);
				if let Some(old_account) = &old_account {
					frame_system::Pallet::<T>::dec_sufficients(old_account);
					AccountToAlias::<T>::remove(old_account);
				}
				frame_system::Pallet::<T>::inc_sufficients(&account);
			}

			AccountToAlias::<T>::insert(&account, &rev_ca);
			AliasToAccount::<T>::insert(&rev_ca.ca, &account);

			if old_account.is_none() || needs_revision {
				Ok(Pays::No.into())
			} else {
				Ok(Pays::Yes.into())
			}
		}

		/// Remove the mapping from a particular alias to its registered account.
		#[pallet::call_index(2)]
		pub fn unset_alias_account(origin: OriginFor<T>) -> DispatchResult {
			let alias = Self::ensure_personal_alias(origin)?;
			let account = AliasToAccount::<T>::take(&alias).ok_or(Error::<T>::InvalidAccount)?;
			AccountToAlias::<T>::remove(&account);
			frame_system::Pallet::<T>::dec_sufficients(&account);

			Ok(())
		}

		/// Recognize a set of people without any additional checks.
		///
		/// The people are identified by the provided list of keys and will each be assigned, in
		/// order, the next available personal ID.
		///
		/// The origin for this call must have root privileges.
		#[pallet::call_index(3)]
		pub fn force_recognize_personhood(
			origin: OriginFor<T>,
			people: Vec<MemberOf<T>>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			for key in people {
				let personal_id = Self::reserve_new_id();
				Self::recognize_personhood(personal_id, Some(key))?;
			}
			Ok(().into())
		}

		/// Set a personal id account.
		///
		/// The account can then be used to sign transactions on behalf of the personal id, and
		/// provide replay protection with the nonce.
		///
		/// This transaction is refunded if successful and no account was previously set for the
		/// personal id.
		///
		/// The call is valid from `call_valid_at` until
		/// `call_valid_at + account_setup_time_tolerance`.
		/// `account_setup_time_tolerance` is a constant available in the metadata.
		///
		/// Parameters:
		/// - `account`: The account to set the alias for.
		/// - `call_valid_at`: The block number when the call becomes valid.
		#[pallet::call_index(4)]
		pub fn set_personal_id_account(
			origin: OriginFor<T>,
			account: T::AccountId,
			call_valid_at: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			let id = Self::ensure_personal_identity(origin)?;
			let now = frame_system::Pallet::<T>::block_number();
			let time_tolerance = Self::account_setup_time_tolerance();
			ensure!(
				call_valid_at <= now && now <= call_valid_at.saturating_add(time_tolerance),
				Error::<T>::TimeOutOfRange
			);
			ensure!(!AccountToPersonalId::<T>::contains_key(&account), Error::<T>::AccountInUse);
			ensure!(!AccountToAlias::<T>::contains_key(&account), Error::<T>::AccountInUse);
			let mut record = People::<T>::get(id).ok_or(Error::<T>::NotPerson)?;
			let pays = if let Some(old_account) = record.account {
				frame_system::Pallet::<T>::dec_sufficients(&old_account);
				AccountToPersonalId::<T>::remove(&old_account);
				Pays::Yes
			} else {
				Pays::No
			};
			record.account = Some(account.clone());
			frame_system::Pallet::<T>::inc_sufficients(&account);
			AccountToPersonalId::<T>::insert(&account, id);
			People::<T>::insert(id, &record);

			Ok(pays.into())
		}

		/// Unset the personal id account.
		#[pallet::call_index(5)]
		pub fn unset_personal_id_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let id = Self::ensure_personal_identity(origin)?;
			let mut record = People::<T>::get(id).ok_or(Error::<T>::NotPerson)?;
			let account = record.account.take().ok_or(Error::<T>::InvalidAccount)?;
			AccountToPersonalId::<T>::take(&account).ok_or(Error::<T>::InvalidAccount)?;
			frame_system::Pallet::<T>::dec_sufficients(&account);
			People::<T>::insert(id, &record);

			Ok(Pays::Yes.into())
		}

		/// Migrate the key for a person who was onboarded and is currently included in a ring. The
		/// migration is not instant as the key replacement and subsequent inclusion in a new ring
		/// root will happen only after the next mutation session.
		#[pallet::call_index(6)]
		pub fn migrate_included_key(
			origin: OriginFor<T>,
			new_key: MemberOf<T>,
		) -> DispatchResultWithPostInfo {
			let id = Self::ensure_personal_identity(origin)?;
			ensure!(!Keys::<T>::contains_key(&new_key), Error::<T>::KeyAlreadyInUse);
			let mut record = People::<T>::get(id).ok_or(Error::<T>::NotPerson)?;
			ensure!(record.key != new_key, Error::<T>::SameKey);
			match &record.position {
				// If the key is already included in a ring, enqueue it for migration during the
				// next mutation session.
				RingPosition::Included { ring_index, ring_position, .. } => {
					// If the person scheduled another migration before, remove the key we are
					// replacing from the key registry.
					if let Some(old_migrated_key) = KeyMigrationQueue::<T>::get(id) {
						Keys::<T>::remove(old_migrated_key);
					}
					// Add this new key to the migration queue.
					KeyMigrationQueue::<T>::insert(id, &new_key);
					// Mark this record as stale.
					record.position = RingPosition::Included {
						ring_index: *ring_index,
						ring_position: *ring_position,
						scheduled_for_removal: true,
					};
					// Update the record.
					People::<T>::insert(id, record);
				},
				// This call accepts migrations only for included keys.
				RingPosition::Onboarding { .. } =>
					return Err(Error::<T>::InvalidKeyMigration.into()),
				// Suspended people shouldn't be able to call this, but protect against this case
				// anyway.
				RingPosition::Suspended => return Err(Error::<T>::Suspended.into()),
			}
			Keys::<T>::insert(new_key, id);

			Ok(().into())
		}

		/// Migrate the key for a person who is currently onboarding. The operation is instant,
		/// replacing the old key in the onboarding queue.
		#[pallet::call_index(7)]
		pub fn migrate_onboarding_key(
			origin: OriginFor<T>,
			new_key: MemberOf<T>,
		) -> DispatchResultWithPostInfo {
			let id = Self::ensure_personal_identity(origin)?;
			ensure!(!Keys::<T>::contains_key(&new_key), Error::<T>::KeyAlreadyInUse);
			let mut record = People::<T>::get(id).ok_or(Error::<T>::NotPerson)?;
			ensure!(record.key != new_key, Error::<T>::SameKey);
			match &record.position {
				// If it's still onboarding, just replace the old key in the queue.
				RingPosition::Onboarding { queue_page } => {
					let mut keys = OnboardingQueue::<T>::get(queue_page);
					if let Some(idx) = keys.iter().position(|k| *k == record.key) {
						// Remove the key that never made it into a ring.
						Keys::<T>::remove(&keys[idx]);
						// Update the key in the queue.
						keys[idx] = new_key.clone();
						OnboardingQueue::<T>::insert(queue_page, keys);
						// Replace the key in the record.
						record.key = new_key.clone();
						// Update the record.
						People::<T>::insert(id, record);
					} else {
						defensive!("No key found at the position in the person record of {}", id);
					}
				},
				// This call accepts migrations only for included keys.
				RingPosition::Included { .. } => return Err(Error::<T>::InvalidKeyMigration.into()),
				// Suspended people shouldn't be able to call this, but protect against this case
				// anyway.
				RingPosition::Suspended => return Err(Error::<T>::Suspended.into()),
			}
			Keys::<T>::insert(new_key, id);

			Ok(().into())
		}

		/// Force set the onboarding size for new people. This call requires root privileges.
		#[pallet::call_index(8)]
		pub fn set_onboarding_size(
			origin: OriginFor<T>,
			onboarding_size: u32,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(
				onboarding_size <= <T as Config>::MaxRingSize::get(),
				Error::<T>::InvalidOnboardingSize
			);
			OnboardingSize::<T>::put(onboarding_size);
			Ok(Pays::No.into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// If the conditions to build a ring are met, this function returns the number of people to
		/// be included in a `build_ring` call. Otherwise, this function returns `None`.
		pub(crate) fn should_build_ring(ring_index: RingIndex, limit: u32) -> Option<u32> {
			// Ring root cannot be built while there are people to remove.
			if !RingsState::<T>::get().append_only() {
				return None;
			}
			// Suspended people should be removed from the ring before building it.
			if PendingSuspensions::<T>::contains_key(ring_index) {
				return None;
			}

			let ring_status = RingKeysStatus::<T>::get(ring_index);
			let not_included_count = ring_status.total.saturating_sub(ring_status.included);
			let to_include = not_included_count.min(limit);
			// There must be at least one person waiting to be included to build the ring.
			if to_include == 0 {
				return None;
			}

			Some(to_include)
		}

		/// If the conditions to onboard new people into rings are met, this function returns the
		/// number of people to be onboarded from the queue in a `onboard_people` call along with a
		/// flag which states whether the call will completely populate the ring. Otherwise, this
		/// function returns `None`.
		fn should_onboard_people(
			ring_index: RingIndex,
			ring_status: &RingStatus,
			open_slots: u32,
			available_for_inclusion: u32,
			onboarding_size: u32,
		) -> Option<(u32, bool)> {
			// People cannot be onboarded while suspensions are ongoing.
			if !RingsState::<T>::get().append_only() {
				return None;
			}

			// Suspended people should be removed from the ring before building it.
			if PendingSuspensions::<T>::contains_key(ring_index) {
				return None;
			}

			let to_include = available_for_inclusion.min(open_slots);
			// If everything is already included, nothing to do.
			if to_include == 0 {
				return None;
			}

			// Here we check we have enough items in the queue so that the onboarding group size is
			// respected, but also that we can support another queue of at least onboarding size
			// in a future call.
			let can_onboard_with_cohort = to_include >= onboarding_size &&
				ring_status.total.saturating_add(to_include.saturated_into()) <=
					T::MaxRingSize::get().saturating_sub(onboarding_size);
			// If this call completely fills the ring, no onboarding rule enforcement will be
			// necessary.
			let ring_filled = open_slots == to_include;

			let should_onboard = ring_filled || can_onboard_with_cohort;
			if !should_onboard {
				return None;
			}

			Some((to_include, ring_filled))
		}

		/// Returns whether suspensions are allowed and necessary for a given ring index.
		pub(crate) fn should_remove_suspended_keys(
			ring_index: RingIndex,
			check_rings_state: bool,
		) -> bool {
			if check_rings_state && !RingsState::<T>::get().append_only() {
				return false;
			}
			let suspended_count = PendingSuspensions::<T>::decode_len(ring_index).unwrap_or(0);
			// There must be keys to suspend.
			if suspended_count == 0 {
				return false;
			}

			true
		}

		/// Function that checks if the top two onboarding queue pages can be merged into a single
		/// page to defragment the list. This function returns an action to take following the
		/// check. In case a merge is needed, the following information is provided, in order:
		/// * The initial `head` of the queue - will need to remove the page at this index in case
		///   the merge is performed.
		/// * The new `head` of the queue.
		/// * The keys on the first page of the queue.
		/// * The keys on the second page of the queue.
		pub(crate) fn should_merge_queue_pages() -> QueueMergeAction<T> {
			let (initial_head, tail) = QueuePageIndices::<T>::get();
			let first_key_page = OnboardingQueue::<T>::get(initial_head);
			// A `head != tail` condition should mean that there is at least one more page
			// following this one.
			if initial_head == tail {
				return QueueMergeAction::NoAction;
			}
			let new_head = initial_head.checked_add(1).unwrap_or(0);
			let second_key_page = OnboardingQueue::<T>::get(new_head);

			let page_size = T::OnboardingQueuePageSize::get();
			// Make sure the pages can be merged.
			if first_key_page.len().saturating_add(second_key_page.len()) > page_size as usize {
				return QueueMergeAction::NoAction;
			}

			QueueMergeAction::Merge { initial_head, new_head, first_key_page, second_key_page }
		}

		/// Build a ring root by adding all people who were assigned to this ring but not yet
		/// included into the root.
		pub(crate) fn build_ring(ring_index: RingIndex, to_include: u32) -> DispatchResult {
			let (keys, mut ring_status) = Self::ring_keys_and_info(ring_index);
			// Get the current ring, and check it should be rebuilt.
			// Return the next revision.
			let (next_revision, mut intermediate) =
				if let Some(existing_root) = Root::<T>::get(ring_index) {
					// We should build a new ring. Return the new revision number we should use.
					(
						existing_root.revision.checked_add(1).ok_or(ArithmeticError::Overflow)?,
						existing_root.intermediate,
					)
				} else {
					// No ring has been built at this index, so we start at revision 0.
					(0, T::Crypto::start_members())
				};

			// Push the members.
			T::Crypto::push_members(
				&mut intermediate,
				keys.iter()
					.skip(ring_status.included as usize)
					.take(to_include as usize)
					.cloned(),
				Self::fetch_chunks,
			)
			.defensive()
			.map_err(|_| Error::<T>::CouldNotPush)?;

			// By the end of the loop, we have included the maximum number of keys in the vector.
			ring_status.included = ring_status.included.saturating_add(to_include);
			RingKeysStatus::<T>::insert(ring_index, ring_status);

			// We create the root after pushing all members.
			let root = T::Crypto::finish_members(intermediate.clone());
			let ring_root = RingRoot { root, revision: next_revision, intermediate };
			Root::<T>::insert(ring_index, ring_root);
			Ok(())
		}

		/// Onboard as many people as possible into the available ring.
		///
		/// This function returns an error if there aren't enough people in the onboarding queue to
		/// complete the operation, or if the number of remaining open slots in the ring would be
		/// below the minimum onboarding size allowed.
		#[transactional]
		pub(crate) fn onboard_people() -> DispatchResult {
			// Get the keys for this ring, and make sure that the ring is full before we build it.
			let (top_ring_index, mut keys) = Self::available_ring();
			let mut ring_status = RingKeysStatus::<T>::get(top_ring_index);
			defensive_assert!(
				keys.len() == ring_status.total as usize,
				"Stored key count doesn't match the actual length"
			);

			let keys_len = keys.len() as u32;
			let open_slots = T::MaxRingSize::get().saturating_sub(keys_len);

			let (mut head, tail) = QueuePageIndices::<T>::get();
			let old_head = head;
			let mut keys_to_include: Vec<MemberOf<T>> =
				OnboardingQueue::<T>::take(head).into_inner();

			// A `head != tail` condition should mean that there is at least one key in the page
			// following this one.
			if keys_to_include.len() < open_slots as usize && head != tail {
				head = head.checked_add(1).unwrap_or(0);
				let second_key_page = OnboardingQueue::<T>::take(head);
				defensive_assert!(!second_key_page.is_empty());
				keys_to_include.extend(second_key_page.into_iter());
			}

			let onboarding_size = OnboardingSize::<T>::get();

			let (to_include, ring_filled) = Self::should_onboard_people(
				top_ring_index,
				&ring_status,
				open_slots,
				keys_to_include.len().saturated_into(),
				onboarding_size,
			)
			.ok_or(Error::<T>::Incomplete)?;

			let mut remaining_keys = keys_to_include.split_off(to_include as usize);
			for key in keys_to_include.into_iter() {
				let personal_id = Keys::<T>::get(&key).defensive().ok_or(Error::<T>::NotPerson)?;
				let mut record =
					People::<T>::get(personal_id).defensive().ok_or(Error::<T>::KeyNotFound)?;
				record.position = RingPosition::Included {
					ring_index: top_ring_index,
					ring_position: keys.len().saturated_into(),
					scheduled_for_removal: false,
				};
				People::<T>::insert(personal_id, record);
				keys.try_push(key).defensive().map_err(|_| Error::<T>::TooManyMembers)?;
			}
			RingKeys::<T>::insert(top_ring_index, keys);
			ActiveMembers::<T>::mutate(|active| *active = active.saturating_add(to_include));
			ring_status.total = ring_status.total.saturating_add(to_include);
			RingKeysStatus::<T>::insert(top_ring_index, ring_status);

			// Update the top ring index if this onboarding round filled the current ring.
			if ring_filled {
				CurrentRingIndex::<T>::mutate(|i| i.saturating_inc());
			}

			if remaining_keys.len() > T::OnboardingQueuePageSize::get() as usize {
				let split_idx =
					remaining_keys.len().saturating_sub(T::OnboardingQueuePageSize::get() as usize);
				let second_page_keys: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize> =
					remaining_keys
						.split_off(split_idx)
						.try_into()
						.expect("the list shrunk so it must fit; qed");
				let remaining_keys: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize> =
					remaining_keys.try_into().expect("the list shrunk so it must fit; qed");
				OnboardingQueue::<T>::insert(old_head, remaining_keys);
				OnboardingQueue::<T>::insert(head, second_page_keys);
				QueuePageIndices::<T>::put((old_head, tail));
			} else if !remaining_keys.is_empty() {
				let remaining_keys: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize> =
					remaining_keys.try_into().expect("the list shrunk so it must fit; qed");
				OnboardingQueue::<T>::insert(head, remaining_keys);
				QueuePageIndices::<T>::put((head, tail));
			} else {
				// We have nothing to put back into the queue, so if this isn't the last page, move
				// the head to the next page of the queue.
				if head != tail {
					head = head.checked_add(1).unwrap_or(0);
				}
				QueuePageIndices::<T>::put((head, tail));
			}
			Ok(())
		}

		fn derivative_call(
			mut origin: OriginFor<T>,
			local_origin: Origin,
			call: <T as frame_system::Config>::RuntimeCall,
			derivation_weight: Weight,
		) -> DispatchResultWithPostInfo {
			origin.set_caller_from(<T::RuntimeOrigin as OriginTrait>::PalletsOrigin::from(
				local_origin,
			));
			let info = call.get_dispatch_info();
			let result = call.dispatch(origin);
			let weight = derivation_weight.saturating_add(extract_actual_weight(&result, &info));
			result
				.map(|p| PostDispatchInfo { actual_weight: Some(weight), pays_fee: p.pays_fee })
				.map_err(|mut err| {
					err.post_info = Some(weight).into();
					err
				})
		}

		/// Ensure that the origin `o` represents a person.
		/// Returns `Ok` with the base identity of the person on success.
		pub fn ensure_personal_identity(
			origin: T::RuntimeOrigin,
		) -> Result<PersonalId, DispatchError> {
			Ok(ensure_personal_identity(origin.into_caller())?)
		}

		/// Ensure that the origin `o` represents a person.
		/// Returns `Ok` with the alias of the person together with the context in which it can
		/// be used on success.
		pub fn ensure_personal_alias(
			origin: T::RuntimeOrigin,
		) -> Result<ContextualAlias, DispatchError> {
			Ok(ensure_personal_alias(origin.into_caller())?)
		}

		/// Ensure that the origin `o` represents a person.
		/// On success returns `Ok` with the revised alias of the person together with the context
		/// in which it can be used and the revision of the ring the person is in.
		pub fn ensure_revised_personal_alias(
			origin: T::RuntimeOrigin,
		) -> Result<RevisedContextualAlias, DispatchError> {
			Ok(ensure_revised_personal_alias(origin.into_caller())?)
		}

		// This function always returns the ring index and the keys for the ring which is currently
		// accepting new members.
		pub fn available_ring() -> (RingIndex, BoundedVec<MemberOf<T>, T::MaxRingSize>) {
			let mut current_ring_index = CurrentRingIndex::<T>::get();
			let mut current_keys = RingKeys::<T>::get(current_ring_index);

			defensive_assert!(
				!current_keys.is_full(),
				"Something bad happened inside the STF, where the current keys are full, but we should have incremented in that case."
			);

			// This condition shouldn't be reached, but we handle the error just in case.
			if current_keys.is_full() {
				current_ring_index.saturating_inc();
				CurrentRingIndex::<T>::put(current_ring_index);
				current_keys = RingKeys::<T>::get(current_ring_index);
			}

			defensive_assert!(
				!current_keys.is_full(),
				"Something bad happened inside the STF, where the current key and next key are both full. Nothing we can do here."
			);

			(current_ring_index, current_keys)
		}

		// This allows us to associate a key with a person.
		pub fn do_insert_key(who: PersonalId, key: MemberOf<T>) -> DispatchResult {
			// If the key is already in use by another person then error.
			ensure!(!Keys::<T>::contains_key(&key), Error::<T>::KeyAlreadyInUse);
			// This is a first time key, so it must be reserved.
			ensure!(
				ReservedPersonalId::<T>::take(who).is_some(),
				Error::<T>::PersonalIdNotReservedOrNotRecognized
			);

			Self::push_to_onboarding_queue(who, key, None)
		}

		// Enqueue personhood suspensions. This function can be called multiple times until all
		// people are marked as suspended, but it can only happen while there is a mutation session
		// in progress.
		pub fn queue_personhood_suspensions(suspensions: &[PersonalId]) -> DispatchResult {
			ensure!(RingsState::<T>::get().mutating(), Error::<T>::NoMutationSession);
			for who in suspensions {
				let mut record = People::<T>::get(who).ok_or(Error::<T>::InvalidSuspensions)?;
				match record.position {
					RingPosition::Included { ring_index, ring_position, .. } => {
						let mut suspended_indices = PendingSuspensions::<T>::get(ring_index);
						let Err(insert_idx) = suspended_indices.binary_search(&ring_position)
						else {
							return Err(Error::<T>::KeyAlreadySuspended.into())
						};
						suspended_indices
							.try_insert(insert_idx, ring_position)
							.defensive()
							.map_err(|_| Error::<T>::TooManyMembers)?;
						PendingSuspensions::<T>::insert(ring_index, suspended_indices);
					},
					RingPosition::Onboarding { queue_page } => {
						let mut keys = OnboardingQueue::<T>::get(queue_page);
						let queue_idx = keys.iter().position(|k| *k == record.key);
						if let Some(idx) = queue_idx {
							// It is expensive to shift the whole vec in the worst case to remove a
							// suspended person from onboarding, but the pages will be small and
							// suspension of people who are not yet onboarded is supposed to be
							// extremely rare if not impossible as the pallet hooks should have
							// plenty of time to include someone recognized before the beginning of
							// the next suspension round. The only legitimate case when this could
							// happen is if someone is sitting in the onboarding queue for a long
							// time and cannot be included because not enough people are joining,
							// but it should be a rare case.
							keys.remove(idx);
							OnboardingQueue::<T>::insert(queue_page, keys);
						} else {
							defensive!(
								"No key found at the position in the person record of {}",
								who
							);
						}
					},
					RingPosition::Suspended => {
						defensive!("Suspension queued for person {} while already suspended", who);
					},
				}

				record.position = RingPosition::Suspended;
				if let Some(account) = record.account {
					AccountToPersonalId::<T>::remove(account);
					record.account = None;
				}

				People::<T>::insert(who, record);
			}

			Ok(())
		}

		// Resume someone's personhood. This assumes that their personhood is currently suspended,
		// so the person was previously recognized.
		pub fn resume_personhood(who: PersonalId) -> DispatchResult {
			let record = People::<T>::get(who).ok_or(Error::<T>::NotPerson)?;
			ensure!(record.position.suspended(), Error::<T>::NotSuspended);
			ensure!(Keys::<T>::get(&record.key) == Some(who), Error::<T>::NoKey);

			Self::push_to_onboarding_queue(who, record.key, record.account)
		}

		fn push_to_onboarding_queue(
			who: PersonalId,
			key: MemberOf<T>,
			account: Option<T::AccountId>,
		) -> DispatchResult {
			let (head, mut tail) = QueuePageIndices::<T>::get();
			let mut keys = OnboardingQueue::<T>::get(tail);
			if let Err(k) = keys.try_push(key.clone()) {
				tail = tail.checked_add(1).unwrap_or(0);
				ensure!(tail != head, Error::<T>::TooManyMembers);
				keys = alloc::vec![k].try_into().expect("must be able to hold one key; qed");
			};

			let record = PersonRecord {
				key,
				position: RingPosition::Onboarding { queue_page: tail },
				account,
			};
			Keys::<T>::insert(&record.key, who);
			People::<T>::insert(who, &record);
			Self::deposit_event(Event::<T>::PersonOnboarding { who, key: record.key });

			QueuePageIndices::<T>::put((head, tail));
			OnboardingQueue::<T>::insert(tail, keys);
			Ok(())
		}

		/// Fetch the keys in a ring along with stored inclusion information.
		pub fn ring_keys_and_info(
			ring_index: RingIndex,
		) -> (BoundedVec<MemberOf<T>, T::MaxRingSize>, RingStatus) {
			let keys = RingKeys::<T>::get(ring_index);
			let ring_status = RingKeysStatus::<T>::get(ring_index);
			defensive_assert!(
				keys.len() == ring_status.total as usize,
				"Stored key count doesn't match the actual length"
			);
			(keys, ring_status)
		}

		// Given a range, returns the list of chunks that maps to the keys at those indices.
		pub(crate) fn fetch_chunks(
			range: Range<usize>,
		) -> Result<Vec<<T::Crypto as GenerateVerifiable>::StaticChunk>, ()> {
			let chunk_page_size = T::ChunkPageSize::get();
			let expected_len = range.end.saturating_sub(range.start);
			let mut page_idx = range.start.checked_div(chunk_page_size as usize).ok_or(())?;
			let mut chunks: Vec<_> = Chunks::<T>::get(page_idx.saturated_into::<u32>())
				.defensive()
				.ok_or(())?
				.into_iter()
				.skip(range.start % chunk_page_size as usize)
				.take(expected_len)
				.collect();
			while chunks.len() < expected_len {
				// Condition to eventually break out of a possible infinite loop in case
				// storage is full of empty chunk pages.
				page_idx = page_idx.checked_add(1).ok_or(())?;
				let page =
					Chunks::<T>::get(page_idx.saturated_into::<u32>()).defensive().ok_or(())?;
				chunks.extend(
					page.into_inner().into_iter().take(expected_len.saturating_sub(chunks.len())),
				);
			}

			Ok(chunks)
		}

		/// Migrates keys that people intend to replace with other keys, if possible. As this
		/// function mutates a fair amount of storage, it comes with a weight meter to limit on the
		/// number of keys to migrate in one call.
		pub(crate) fn migrate_keys(meter: &mut WeightMeter) {
			let mut drain = KeyMigrationQueue::<T>::drain();
			loop {
				// Ensure we have enough weight to look into `KeyMigrationQueue` and perform a
				// removal.
				let weight = T::WeightInfo::migrate_keys_single_included_key()
					.saturating_add(T::DbWeight::get().reads_writes(1, 1));
				if !meter.can_consume(weight) {
					return;
				}

				let op_res = with_storage_layer::<bool, DispatchError, _>(|| match drain.next() {
					Some((id, new_key)) =>
						Self::migrate_keys_single_included_key(id, new_key).map(|_| false),
					None => {
						let rings_state = RingsState::<T>::get()
							.end_key_migration()
							.map_err(|_| Error::<T>::NoMutationSession)?;
						RingsState::<T>::put(rings_state);
						meter.consume(T::DbWeight::get().reads_writes(1, 1));
						Ok(true)
					},
				});
				match op_res {
					Ok(false) => meter.consume(weight),
					Ok(true) => {
						// Read on `KeyMigrationQueue`.
						meter.consume(T::DbWeight::get().reads(1));
						break
					},
					Err(e) => {
						meter.consume(weight);
						log::error!(target: LOG_TARGET, "failed to migrate keys: {:?}", e);
						break;
					},
				}
			}
		}

		/// A single iteration of the key migration process where an included key marked for
		/// suspension is being removed from a ring.
		pub(crate) fn migrate_keys_single_included_key(
			id: PersonalId,
			new_key: MemberOf<T>,
		) -> DispatchResult {
			if let Some(record) = People::<T>::get(id) {
				let RingPosition::Included {
					ring_index,
					ring_position,
					scheduled_for_removal: true,
				} = record.position
				else {
					Keys::<T>::remove(new_key);
					return Ok(())
				};
				let mut suspended_indices = PendingSuspensions::<T>::get(ring_index);
				let Err(insert_idx) = suspended_indices.binary_search(&ring_position) else {
					log::info!(target: LOG_TARGET, "key migration for person {} skipped as the person's key was already suspended", id);
					return Ok(());
				};
				suspended_indices
					.try_insert(insert_idx, ring_position)
					.map_err(|_| Error::<T>::TooManyMembers)?;
				PendingSuspensions::<T>::insert(ring_index, suspended_indices);
				Keys::<T>::remove(&record.key);
				Self::push_to_onboarding_queue(id, new_key, record.account)?;
			} else {
				log::info!(target: LOG_TARGET, "key migration for person {} skipped as no record was found", id);
			}
			Ok(())
		}

		/// Removes people's keys marked as suspended or inactive from a ring with a given index.
		pub(crate) fn remove_suspended_keys(ring_index: RingIndex) -> Weight {
			let keys = RingKeys::<T>::get(ring_index);
			let keys_len = keys.len();
			let suspended_indices = PendingSuspensions::<T>::get(ring_index);
			// Construct the new keys map by skipping the suspended keys. This should prevent
			// reallocations in the `Vec` which happens with `remove`.
			let mut new_keys: BoundedVec<MemberOf<T>, T::MaxRingSize> = Default::default();
			let mut j = 0;
			for (i, key) in keys.into_iter().enumerate() {
				if j < suspended_indices.len() && i == suspended_indices[j] as usize {
					j += 1;
				} else if new_keys
					.try_push(key)
					.defensive_proof("cannot move more ring members than the max ring size; qed")
					.is_err()
				{
					return T::WeightInfo::remove_suspended_people(
						keys_len.try_into().unwrap_or(u32::MAX),
					);
				}
			}

			let suspended_count = RingKeysStatus::<T>::mutate(ring_index, |ring_status| {
				let new_total = new_keys.len().saturated_into();
				let suspended_count = ring_status.total.saturating_sub(new_total);
				ring_status.total = new_total;
				ring_status.included = 0;
				suspended_count
			});
			ActiveMembers::<T>::mutate(|active| *active = active.saturating_sub(suspended_count));
			RingKeys::<T>::insert(ring_index, new_keys);
			Root::<T>::mutate(ring_index, |maybe_root| {
				if let Some(root) = maybe_root {
					// The revision will be incremented on the next call of `build_ring`. The
					// current root is preserved.
					root.intermediate = T::Crypto::start_members();
				}
			});

			// Make sure to remove the entry from the map so that the pallet hooks don't iterate
			// over it.
			PendingSuspensions::<T>::remove(ring_index);
			T::WeightInfo::remove_suspended_people(keys_len.try_into().unwrap_or(u32::MAX))
		}

		/// Merges the two pages at the front of the onboarding queue. After a round of suspensions,
		/// it is possible for the second page of the onboarding queue to be left with few members
		/// such that, if the first page also has few members, the total count is below the required
		/// onboarding size, thus stalling the queue. This function fixes this by moving the people
		/// from the first page to the front of the second page, defragmenting the queue.
		///
		/// If the operation fails, the storage is rolled back.
		pub(crate) fn merge_queue_pages(
			initial_head: u32,
			new_head: u32,
			mut first_key_page: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize>,
			second_key_page: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize>,
		) {
			let op_res = with_storage_layer::<(), DispatchError, _>(|| {
				// Update the records of the people in the first page.
				for key in first_key_page.iter() {
					let personal_id =
						Keys::<T>::get(key).defensive().ok_or(Error::<T>::NotPerson)?;
					let mut record =
						People::<T>::get(personal_id).defensive().ok_or(Error::<T>::KeyNotFound)?;
					record.position = RingPosition::Onboarding { queue_page: new_head };
					People::<T>::insert(personal_id, record);
				}

				first_key_page
					.try_extend(second_key_page.into_iter())
					.defensive()
					.map_err(|_| Error::<T>::TooManyMembers)?;
				OnboardingQueue::<T>::remove(initial_head);
				OnboardingQueue::<T>::insert(new_head, first_key_page);
				QueuePageIndices::<T>::mutate(|(h, _)| *h = new_head);
				Ok(())
			});
			if let Err(e) = op_res {
				log::error!(target: LOG_TARGET, "failed to merge queue pages: {:?}", e);
			}
		}
	}

	impl<T: Config> AddOnlyPeopleTrait for Pallet<T> {
		type Member = MemberOf<T>;

		fn reserve_new_id() -> PersonalId {
			let new_id = NextPersonalId::<T>::mutate(|id| {
				let new_id = *id;
				id.saturating_inc();
				new_id
			});
			ReservedPersonalId::<T>::insert(new_id, ());
			new_id
		}

		fn cancel_id_reservation(personal_id: PersonalId) -> Result<(), DispatchError> {
			ReservedPersonalId::<T>::take(personal_id).ok_or(Error::<T>::PersonalIdNotReserved)?;
			Ok(())
		}

		fn renew_id_reservation(personal_id: PersonalId) -> Result<(), DispatchError> {
			if NextPersonalId::<T>::get() <= personal_id ||
				People::<T>::contains_key(personal_id) ||
				ReservedPersonalId::<T>::contains_key(personal_id)
			{
				return Err(Error::<T>::PersonalIdReservationCannotRenew.into());
			}
			ReservedPersonalId::<T>::insert(personal_id, ());
			Ok(())
		}

		fn recognize_personhood(
			who: PersonalId,
			maybe_key: Option<MemberOf<T>>,
		) -> Result<(), DispatchError> {
			match maybe_key {
				Some(key) => Self::do_insert_key(who, key),
				None => Self::resume_personhood(who),
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		type Secret = <<T as Config>::Crypto as GenerateVerifiable>::Secret;

		#[cfg(feature = "runtime-benchmarks")]
		fn mock_key(who: PersonalId) -> (Self::Member, Self::Secret) {
			let mut buf = [0u8; 32];
			buf[..core::mem::size_of::<PersonalId>()].copy_from_slice(&who.to_le_bytes()[..]);
			let secret = T::Crypto::new_secret(buf);
			(T::Crypto::member_from_secret(&secret), secret)
		}
	}

	impl<T: Config> PeopleTrait for Pallet<T> {
		fn suspend_personhood(suspensions: &[PersonalId]) -> DispatchResult {
			Self::queue_personhood_suspensions(suspensions)
		}
		fn start_people_set_mutation_session() -> DispatchResult {
			let current_state = RingsState::<T>::get();
			RingsState::<T>::put(
				current_state
					.start_mutation_session()
					.map_err(|_| Error::<T>::CouldNotStartMutationSession)?,
			);
			Ok(())
		}
		fn end_people_set_mutation_session() -> DispatchResult {
			let current_state = RingsState::<T>::get();
			RingsState::<T>::put(
				current_state
					.end_mutation_session()
					.map_err(|_| Error::<T>::NoMutationSession)?,
			);
			Ok(())
		}
	}

	/// Ensure that the origin `o` represents an extrinsic (i.e. transaction) from a personal
	/// identity. Returns `Ok` with the personal identity that signed the extrinsic or an `Err`
	/// otherwise.
	pub fn ensure_personal_identity<OuterOrigin>(o: OuterOrigin) -> Result<PersonalId, BadOrigin>
	where
		OuterOrigin: TryInto<Origin, Error = OuterOrigin>,
	{
		match o.try_into() {
			Ok(Origin::PersonalIdentity(m)) => Ok(m),
			_ => Err(BadOrigin),
		}
	}

	/// Ensure that the origin `o` represents an extrinsic (i.e. transaction) from a personal alias.
	/// Returns `Ok` with the personal alias that signed the extrinsic or an `Err` otherwise.
	pub fn ensure_personal_alias<OuterOrigin>(o: OuterOrigin) -> Result<ContextualAlias, BadOrigin>
	where
		OuterOrigin: TryInto<Origin, Error = OuterOrigin>,
	{
		match o.try_into() {
			Ok(Origin::PersonalAlias(rev_ca)) => Ok(rev_ca.ca),
			_ => Err(BadOrigin),
		}
	}

	/// Guard to ensure that the given origin is a person. The underlying identity of the person is
	/// provided on success.
	pub struct EnsurePersonalIdentity<T>(PhantomData<T>);
	impl<T: Config> EnsureOrigin<OriginFor<T>> for EnsurePersonalIdentity<T> {
		type Success = PersonalId;

		fn try_origin(o: OriginFor<T>) -> Result<Self::Success, OriginFor<T>> {
			ensure_personal_identity(o.clone().into_caller()).map_err(|_| o)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<OriginFor<T>, ()> {
			Ok(Origin::PersonalIdentity(0).into())
		}
	}

	frame_support::impl_ensure_origin_with_arg_ignoring_arg! {
		impl<{ T: Config, A }>
			EnsureOriginWithArg< OriginFor<T>, A> for EnsurePersonalIdentity<T>
		{}
	}

	impl<T: Config> CountedMembers for EnsurePersonalIdentity<T> {
		fn active_count(&self) -> u32 {
			Keys::<T>::count()
		}
	}

	/// Guard to ensure that the given origin is a person. The contextual alias of the person is
	/// provided on success.
	pub struct EnsurePersonalAlias<T>(PhantomData<T>);
	impl<T: Config> EnsureOrigin<OriginFor<T>> for EnsurePersonalAlias<T> {
		type Success = ContextualAlias;

		fn try_origin(o: OriginFor<T>) -> Result<Self::Success, OriginFor<T>> {
			ensure_personal_alias(o.clone().into_caller()).map_err(|_| o)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<OriginFor<T>, ()> {
			Ok(Origin::PersonalAlias(RevisedContextualAlias {
				revision: 0,
				ring: 0,
				ca: ContextualAlias { alias: [1; 32], context: [0; 32] },
			})
			.into())
		}
	}

	frame_support::impl_ensure_origin_with_arg_ignoring_arg! {
		impl<{ T: Config, A }>
			EnsureOriginWithArg< OriginFor<T>, A> for EnsurePersonalAlias<T>
		{}
	}

	impl<T: Config> CountedMembers for EnsurePersonalAlias<T> {
		fn active_count(&self) -> u32 {
			ActiveMembers::<T>::get()
		}
	}

	/// Guard to ensure that the given origin is a person. The alias of the person within the
	/// context provided as an argument is returned on success.
	pub struct EnsurePersonalAliasInContext<T>(PhantomData<T>);
	impl<T: Config> EnsureOriginWithArg<OriginFor<T>, Context> for EnsurePersonalAliasInContext<T> {
		type Success = Alias;

		fn try_origin(o: OriginFor<T>, arg: &Context) -> Result<Self::Success, OriginFor<T>> {
			match ensure_personal_alias(o.clone().into_caller()) {
				Ok(ca) if &ca.context == arg => Ok(ca.alias),
				_ => Err(o),
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin(context: &Context) -> Result<OriginFor<T>, ()> {
			Ok(Origin::PersonalAlias(RevisedContextualAlias {
				revision: 0,
				ring: 0,
				ca: ContextualAlias { alias: [1; 32], context: *context },
			})
			.into())
		}
	}

	impl<T: Config> CountedMembers for EnsurePersonalAliasInContext<T> {
		fn active_count(&self) -> u32 {
			ActiveMembers::<T>::get()
		}
	}

	/// Ensure that the origin `o` represents an extrinsic (i.e. transaction) from a personal alias
	/// with revision information.
	///
	/// Returns `Ok` with the revised personal alias that signed the extrinsic or an `Err`
	/// otherwise.
	pub fn ensure_revised_personal_alias<OuterOrigin>(
		o: OuterOrigin,
	) -> Result<RevisedContextualAlias, BadOrigin>
	where
		OuterOrigin: TryInto<Origin, Error = OuterOrigin>,
	{
		match o.try_into() {
			Ok(Origin::PersonalAlias(rev_ca)) => Ok(rev_ca),
			_ => Err(BadOrigin),
		}
	}

	/// Guard to ensure that the given origin is a person.
	///
	/// The revised contextual alias of the person is provided on success. The revision can be used
	/// to tell in the future if an alias may have been suspended. See [`RevisedContextualAlias`].
	pub struct EnsureRevisedPersonalAlias<T>(PhantomData<T>);
	impl<T: Config> EnsureOrigin<OriginFor<T>> for EnsureRevisedPersonalAlias<T> {
		type Success = RevisedContextualAlias;

		fn try_origin(o: OriginFor<T>) -> Result<Self::Success, OriginFor<T>> {
			ensure_revised_personal_alias(o.clone().into_caller()).map_err(|_| o)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<OriginFor<T>, ()> {
			Ok(Origin::PersonalAlias(RevisedContextualAlias {
				revision: 0,
				ring: 0,
				ca: ContextualAlias { alias: [1; 32], context: [0; 32] },
			})
			.into())
		}
	}

	frame_support::impl_ensure_origin_with_arg_ignoring_arg! {
		impl<{ T: Config, A }>
			EnsureOriginWithArg< OriginFor<T>, A> for EnsureRevisedPersonalAlias<T>
		{}
	}

	impl<T: Config> CountedMembers for EnsureRevisedPersonalAlias<T> {
		fn active_count(&self) -> u32 {
			ActiveMembers::<T>::get()
		}
	}

	/// Guard to ensure that the given origin is a person.
	///
	/// The revised alias of the person within the context provided as an argument is returned on
	/// success. The revision can be used to tell in the future if an alias may have been suspended.
	/// See [`RevisedAlias`].
	pub struct EnsureRevisedPersonalAliasInContext<T>(PhantomData<T>);
	impl<T: Config> EnsureOriginWithArg<OriginFor<T>, Context>
		for EnsureRevisedPersonalAliasInContext<T>
	{
		type Success = RevisedAlias;

		fn try_origin(o: OriginFor<T>, arg: &Context) -> Result<Self::Success, OriginFor<T>> {
			match ensure_revised_personal_alias(o.clone().into_caller()) {
				Ok(ca) if &ca.ca.context == arg =>
					Ok(RevisedAlias { revision: ca.revision, ring: ca.ring, alias: ca.ca.alias }),
				_ => Err(o),
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin(context: &Context) -> Result<OriginFor<T>, ()> {
			Ok(Origin::PersonalAlias(RevisedContextualAlias {
				revision: 0,
				ring: 0,
				ca: ContextualAlias { alias: [1; 32], context: *context },
			})
			.into())
		}
	}

	impl<T: Config> CountedMembers for EnsureRevisedPersonalAliasInContext<T> {
		fn active_count(&self) -> u32 {
			ActiveMembers::<T>::get()
		}
	}
}
