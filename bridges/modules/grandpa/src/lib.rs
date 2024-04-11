// Copyright (C) Parity Technologies (UK) Ltd.
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
//! bug causing resulting in an equivocation. Such events are outside the scope of this pallet.
//! Shall the fork occur on the bridged chain governance intervention will be required to
//! re-initialize the bridge and track the right fork.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

pub use storage_types::StoredAuthoritySet;

use bp_header_chain::{
	justification::GrandpaJustification, AuthoritySet, ChainWithGrandpa, GrandpaConsensusLogReader,
	HeaderChain, InitializationData, StoredHeaderData, StoredHeaderDataBuilder,
	StoredHeaderGrandpaInfo,
};
use bp_runtime::{BlockNumberOf, HashOf, HasherOf, HeaderId, HeaderOf, OwnedBridgeModule};
use frame_support::{dispatch::PostDispatchInfo, ensure, DefaultNoBound};
use sp_runtime::{
	traits::{Header as HeaderT, Zero},
	SaturatedConversion,
};
use sp_std::{boxed::Box, convert::TryInto, prelude::*};

mod call_ext;
#[cfg(test)]
mod mock;
mod storage_types;

/// Module, containing weights for this pallet.
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

// Re-export in crate namespace for `construct_runtime!`
pub use call_ext::*;
pub use pallet::*;
pub use weights::WeightInfo;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "runtime::bridge-grandpa";

/// Bridged chain from the pallet configuration.
pub type BridgedChain<T, I> = <T as Config<I>>::BridgedChain;
/// Block number of the bridged chain.
pub type BridgedBlockNumber<T, I> = BlockNumberOf<<T as Config<I>>::BridgedChain>;
/// Block hash of the bridged chain.
pub type BridgedBlockHash<T, I> = HashOf<<T as Config<I>>::BridgedChain>;
/// Block id of the bridged chain.
pub type BridgedBlockId<T, I> = HeaderId<BridgedBlockHash<T, I>, BridgedBlockNumber<T, I>>;
/// Hasher of the bridged chain.
pub type BridgedBlockHasher<T, I> = HasherOf<<T as Config<I>>::BridgedChain>;
/// Header of the bridged chain.
pub type BridgedHeader<T, I> = HeaderOf<<T as Config<I>>::BridgedChain>;
/// Header data of the bridged chain that is stored at this chain by this pallet.
pub type BridgedStoredHeaderData<T, I> =
	StoredHeaderData<BridgedBlockNumber<T, I>, BridgedBlockHash<T, I>>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use bp_runtime::BasicOperatingMode;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The chain we are bridging to here.
		type BridgedChain: ChainWithGrandpa;

		/// Maximal number of "free" mandatory header transactions per block.
		///
		/// To be able to track the bridged chain, the pallet requires all headers that are
		/// changing GRANDPA authorities set at the bridged chain (we call them mandatory).
		/// So it is a common good deed to submit mandatory headers to the pallet. However, if the
		/// bridged chain gets compromised, its validators may generate as many mandatory headers
		/// as they want. And they may fill the whole block (at this chain) for free. This constants
		/// limits number of calls that we may refund in a single block. All calls above this
		/// limit are accepted, but are not refunded.
		#[pallet::constant]
		type MaxFreeMandatoryHeadersPerBlock: Get<u32>;

		/// Maximal number of finalized headers to keep in the storage.
		///
		/// The setting is there to prevent growing the on-chain state indefinitely. Note
		/// the setting does not relate to block numbers - we will simply keep as much items
		/// in the storage, so it doesn't guarantee any fixed timeframe for finality headers.
		///
		/// Incautious change of this constant may lead to orphan entries in the runtime storage.
		#[pallet::constant]
		type HeadersToKeep: Get<u32>;

		/// Weights gathered through benchmarking.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			FreeMandatoryHeadersRemaining::<T, I>::put(T::MaxFreeMandatoryHeadersPerBlock::get());
			Weight::zero()
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			FreeMandatoryHeadersRemaining::<T, I>::kill();
		}
	}

	impl<T: Config<I>, I: 'static> OwnedBridgeModule<T> for Pallet<T, I> {
		const LOG_TARGET: &'static str = LOG_TARGET;
		type OwnerStorage = PalletOwner<T, I>;
		type OperatingMode = BasicOperatingMode;
		type OperatingModeStorage = PalletOperatingMode<T, I>;
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// This call is deprecated and will be removed around May 2024. Use the
		/// `submit_finality_proof_ex` instead. Semantically, this call is an equivalent of the
		/// `submit_finality_proof_ex` call without current authority set id check.
		#[pallet::call_index(0)]
		#[pallet::weight(<T::WeightInfo as WeightInfo>::submit_finality_proof(
			justification.commit.precommits.len().saturated_into(),
			justification.votes_ancestries.len().saturated_into(),
		))]
		#[allow(deprecated)]
		#[deprecated(
			note = "`submit_finality_proof` will be removed in May 2024. Use `submit_finality_proof_ex` instead."
		)]
		pub fn submit_finality_proof(
			origin: OriginFor<T>,
			finality_target: Box<BridgedHeader<T, I>>,
			justification: GrandpaJustification<BridgedHeader<T, I>>,
		) -> DispatchResultWithPostInfo {
			Self::submit_finality_proof_ex(
				origin,
				finality_target,
				justification,
				// the `submit_finality_proof_ex` also reads this value, but it is done from the
				// cache, so we don't treat it as an additional db access
				<CurrentAuthoritySet<T, I>>::get().set_id,
			)
		}

		/// Bootstrap the bridge pallet with an initial header and authority set from which to sync.
		///
		/// The initial configuration provided does not need to be the genesis header of the bridged
		/// chain, it can be any arbitrary header. You can also provide the next scheduled set
		/// change if it is already know.
		///
		/// This function is only allowed to be called from a trusted origin and writes to storage
		/// with practically no checks in terms of the validity of the data. It is important that
		/// you ensure that valid data is being passed in.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().reads_writes(2, 5), DispatchClass::Operational))]
		pub fn initialize(
			origin: OriginFor<T>,
			init_data: super::InitializationData<BridgedHeader<T, I>>,
		) -> DispatchResultWithPostInfo {
			Self::ensure_owner_or_root(origin)?;

			let init_allowed = !<BestFinalized<T, I>>::exists();
			ensure!(init_allowed, <Error<T, I>>::AlreadyInitialized);
			initialize_bridge::<T, I>(init_data.clone())?;

			log::info!(
				target: LOG_TARGET,
				"Pallet has been initialized with the following parameters: {:?}",
				init_data
			);

			Ok(().into())
		}

		/// Change `PalletOwner`.
		///
		/// May only be called either by root, or by `PalletOwner`.
		#[pallet::call_index(2)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_owner(origin: OriginFor<T>, new_owner: Option<T::AccountId>) -> DispatchResult {
			<Self as OwnedBridgeModule<_>>::set_owner(origin, new_owner)
		}

		/// Halt or resume all pallet operations.
		///
		/// May only be called either by root, or by `PalletOwner`.
		#[pallet::call_index(3)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operating_mode(
			origin: OriginFor<T>,
			operating_mode: BasicOperatingMode,
		) -> DispatchResult {
			<Self as OwnedBridgeModule<_>>::set_operating_mode(origin, operating_mode)
		}

		/// Verify a target header is finalized according to the given finality proof. The proof
		/// is assumed to be signed by GRANDPA authorities set with `current_set_id` id.
		///
		/// It will use the underlying storage pallet to fetch information about the current
		/// authorities and best finalized header in order to verify that the header is finalized.
		///
		/// If successful in verification, it will write the target header to the underlying storage
		/// pallet.
		///
		/// The call fails if:
		///
		/// - the pallet is halted;
		///
		/// - the pallet knows better header than the `finality_target`;
		///
		/// - the id of best GRANDPA authority set, known to the pallet is not equal to the
		///   `current_set_id`;
		///
		/// - verification is not optimized or invalid;
		///
		/// - header contains forced authorities set change or change with non-zero delay.
		#[pallet::call_index(4)]
		#[pallet::weight(<T::WeightInfo as WeightInfo>::submit_finality_proof(
			justification.commit.precommits.len().saturated_into(),
			justification.votes_ancestries.len().saturated_into(),
		))]
		pub fn submit_finality_proof_ex(
			origin: OriginFor<T>,
			finality_target: Box<BridgedHeader<T, I>>,
			justification: GrandpaJustification<BridgedHeader<T, I>>,
			current_set_id: sp_consensus_grandpa::SetId,
		) -> DispatchResultWithPostInfo {
			Self::ensure_not_halted().map_err(Error::<T, I>::BridgeModule)?;
			ensure_signed(origin)?;

			let (hash, number) = (finality_target.hash(), *finality_target.number());
			log::trace!(
				target: LOG_TARGET,
				"Going to try and finalize header {:?}",
				finality_target
			);

			// it checks whether the `number` is better than the current best block number
			// and whether the `current_set_id` matches the best known set id
			SubmitFinalityProofHelper::<T, I>::check_obsolete(number, Some(current_set_id))?;

			let authority_set = <CurrentAuthoritySet<T, I>>::get();
			let unused_proof_size = authority_set.unused_proof_size();
			let set_id = authority_set.set_id;
			let authority_set: AuthoritySet = authority_set.into();
			verify_justification::<T, I>(&justification, hash, number, authority_set)?;

			let maybe_new_authority_set =
				try_enact_authority_change::<T, I>(&finality_target, set_id)?;
			let may_refund_call_fee = maybe_new_authority_set.is_some() &&
				// if we have seen too many mandatory headers in this block, we don't want to refund
				Self::free_mandatory_headers_remaining() > 0 &&
				// if arguments out of expected bounds, we don't want to refund
				submit_finality_proof_info_from_args::<T, I>(&finality_target, &justification, Some(current_set_id))
					.fits_limits();
			if may_refund_call_fee {
				FreeMandatoryHeadersRemaining::<T, I>::mutate(|count| {
					*count = count.saturating_sub(1)
				});
			}
			insert_header::<T, I>(*finality_target, hash);
			log::info!(
				target: LOG_TARGET,
				"Successfully imported finalized header with hash {:?}!",
				hash
			);

			// mandatory header is a header that changes authorities set. The pallet can't go
			// further without importing this header. So every bridge MUST import mandatory headers.
			//
			// We don't want to charge extra costs for mandatory operations. So relayer is not
			// paying fee for mandatory headers import transactions.
			//
			// If size/weight of the call is exceeds our estimated limits, the relayer still needs
			// to pay for the transaction.
			let pays_fee = if may_refund_call_fee { Pays::No } else { Pays::Yes };

			// the proof size component of the call weight assumes that there are
			// `MaxBridgedAuthorities` in the `CurrentAuthoritySet` (we use `MaxEncodedLen`
			// estimation). But if their number is lower, then we may "refund" some `proof_size`,
			// making proof smaller and leaving block space to other useful transactions
			let pre_dispatch_weight = T::WeightInfo::submit_finality_proof(
				justification.commit.precommits.len().saturated_into(),
				justification.votes_ancestries.len().saturated_into(),
			);
			let actual_weight = pre_dispatch_weight
				.set_proof_size(pre_dispatch_weight.proof_size().saturating_sub(unused_proof_size));

			Self::deposit_event(Event::UpdatedBestFinalizedHeader {
				number,
				hash,
				grandpa_info: StoredHeaderGrandpaInfo {
					finality_proof: justification,
					new_verification_context: maybe_new_authority_set,
				},
			});

			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee })
		}
	}

	/// Number mandatory headers that we may accept in the current block for free (returning
	/// `Pays::No`).
	///
	/// If the `FreeMandatoryHeadersRemaining` hits zero, all following mandatory headers in the
	/// current block are accepted with fee (`Pays::Yes` is returned).
	///
	/// The `FreeMandatoryHeadersRemaining` is an ephemeral value that is set to
	/// `MaxFreeMandatoryHeadersPerBlock` at each block initialization and is killed on block
	/// finalization. So it never ends up in the storage trie.
	#[pallet::storage]
	#[pallet::whitelist_storage]
	#[pallet::getter(fn free_mandatory_headers_remaining)]
	pub(super) type FreeMandatoryHeadersRemaining<T: Config<I>, I: 'static = ()> =
		StorageValue<_, u32, ValueQuery>;

	/// Hash of the header used to bootstrap the pallet.
	#[pallet::storage]
	pub(super) type InitialHash<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BridgedBlockHash<T, I>, ValueQuery>;

	/// Hash of the best finalized header.
	#[pallet::storage]
	#[pallet::getter(fn best_finalized)]
	pub type BestFinalized<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BridgedBlockId<T, I>, OptionQuery>;

	/// A ring buffer of imported hashes. Ordered by the insertion time.
	#[pallet::storage]
	pub(super) type ImportedHashes<T: Config<I>, I: 'static = ()> = StorageMap<
		Hasher = Identity,
		Key = u32,
		Value = BridgedBlockHash<T, I>,
		QueryKind = OptionQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeHeadersToKeep<T, I>,
	>;

	/// Current ring buffer position.
	#[pallet::storage]
	pub(super) type ImportedHashesPointer<T: Config<I>, I: 'static = ()> =
		StorageValue<_, u32, ValueQuery>;

	/// Relevant fields of imported headers.
	#[pallet::storage]
	pub type ImportedHeaders<T: Config<I>, I: 'static = ()> = StorageMap<
		Hasher = Identity,
		Key = BridgedBlockHash<T, I>,
		Value = BridgedStoredHeaderData<T, I>,
		QueryKind = OptionQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeHeadersToKeep<T, I>,
	>;

	/// The current GRANDPA Authority set.
	#[pallet::storage]
	pub type CurrentAuthoritySet<T: Config<I>, I: 'static = ()> =
		StorageValue<_, StoredAuthoritySet<T, I>, ValueQuery>;

	/// Optional pallet owner.
	///
	/// Pallet owner has a right to halt all pallet operations and then resume it. If it is
	/// `None`, then there are no direct ways to halt/resume pallet operations, but other
	/// runtime methods may still be used to do that (i.e. democracy::referendum to update halt
	/// flag directly or call the `halt_operations`).
	#[pallet::storage]
	pub type PalletOwner<T: Config<I>, I: 'static = ()> =
		StorageValue<_, T::AccountId, OptionQuery>;

	/// The current operating mode of the pallet.
	///
	/// Depending on the mode either all, or no transactions will be allowed.
	#[pallet::storage]
	pub type PalletOperatingMode<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BasicOperatingMode, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		/// Optional module owner account.
		pub owner: Option<T::AccountId>,
		/// Optional module initialization data.
		pub init_data: Option<super::InitializationData<BridgedHeader<T, I>>>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(ref owner) = self.owner {
				<PalletOwner<T, I>>::put(owner);
			}

			if let Some(init_data) = self.init_data.clone() {
				initialize_bridge::<T, I>(init_data).expect("genesis config is correct; qed");
			} else {
				// Since the bridge hasn't been initialized we shouldn't allow anyone to perform
				// transactions.
				<PalletOperatingMode<T, I>>::put(BasicOperatingMode::Halted);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Best finalized chain header has been updated to the header with given number and hash.
		UpdatedBestFinalizedHeader {
			/// Number of the new best finalized header.
			number: BridgedBlockNumber<T, I>,
			/// Hash of the new best finalized header.
			hash: BridgedBlockHash<T, I>,
			/// The Grandpa info associated to the new best finalized header.
			grandpa_info: StoredHeaderGrandpaInfo<BridgedHeader<T, I>>,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The given justification is invalid for the given header.
		InvalidJustification,
		/// The authority set from the underlying header chain is invalid.
		InvalidAuthoritySet,
		/// The header being imported is older than the best finalized header known to the pallet.
		OldHeader,
		/// The scheduled authority set change found in the header is unsupported by the pallet.
		///
		/// This is the case for non-standard (e.g forced) authority set changes.
		UnsupportedScheduledChange,
		/// The pallet is not yet initialized.
		NotInitialized,
		/// The pallet has already been initialized.
		AlreadyInitialized,
		/// Too many authorities in the set.
		TooManyAuthoritiesInSet,
		/// Error generated by the `OwnedBridgeModule` trait.
		BridgeModule(bp_runtime::OwnedBridgeModuleError),
		/// The `current_set_id` argument of the `submit_finality_proof_ex` doesn't match
		/// the id of the current set, known to the pallet.
		InvalidAuthoritySetId,
	}

	/// Check the given header for a GRANDPA scheduled authority set change. If a change
	/// is found it will be enacted immediately.
	///
	/// This function does not support forced changes, or scheduled changes with delays
	/// since these types of changes are indicative of abnormal behavior from GRANDPA.
	///
	/// Returned value will indicate if a change was enacted or not.
	pub(crate) fn try_enact_authority_change<T: Config<I>, I: 'static>(
		header: &BridgedHeader<T, I>,
		current_set_id: sp_consensus_grandpa::SetId,
	) -> Result<Option<AuthoritySet>, DispatchError> {
		// We don't support forced changes - at that point governance intervention is required.
		ensure!(
			GrandpaConsensusLogReader::<BridgedBlockNumber<T, I>>::find_forced_change(
				header.digest()
			)
			.is_none(),
			<Error<T, I>>::UnsupportedScheduledChange
		);

		if let Some(change) =
			GrandpaConsensusLogReader::<BridgedBlockNumber<T, I>>::find_scheduled_change(
				header.digest(),
			) {
			// GRANDPA only includes a `delay` for forced changes, so this isn't valid.
			ensure!(change.delay == Zero::zero(), <Error<T, I>>::UnsupportedScheduledChange);

			// TODO [#788]: Stop manually increasing the `set_id` here.
			let next_authorities = StoredAuthoritySet::<T, I> {
				authorities: change
					.next_authorities
					.try_into()
					.map_err(|_| Error::<T, I>::TooManyAuthoritiesInSet)?,
				set_id: current_set_id + 1,
			};

			// Since our header schedules a change and we know the delay is 0, it must also enact
			// the change.
			<CurrentAuthoritySet<T, I>>::put(&next_authorities);

			log::info!(
				target: LOG_TARGET,
				"Transitioned from authority set {} to {}! New authorities are: {:?}",
				current_set_id,
				current_set_id + 1,
				next_authorities,
			);

			return Ok(Some(next_authorities.into()))
		};

		Ok(None)
	}

	/// Verify a GRANDPA justification (finality proof) for a given header.
	///
	/// Will use the GRANDPA current authorities known to the pallet.
	///
	/// If successful it returns the decoded GRANDPA justification so we can refund any weight which
	/// was overcharged in the initial call.
	pub(crate) fn verify_justification<T: Config<I>, I: 'static>(
		justification: &GrandpaJustification<BridgedHeader<T, I>>,
		hash: BridgedBlockHash<T, I>,
		number: BridgedBlockNumber<T, I>,
		authority_set: bp_header_chain::AuthoritySet,
	) -> Result<(), sp_runtime::DispatchError> {
		use bp_header_chain::justification::verify_justification;

		Ok(verify_justification::<BridgedHeader<T, I>>(
			(hash, number),
			&authority_set.try_into().map_err(|_| <Error<T, I>>::InvalidAuthoritySet)?,
			justification,
		)
		.map_err(|e| {
			log::error!(
				target: LOG_TARGET,
				"Received invalid justification for {:?}: {:?}",
				hash,
				e,
			);
			<Error<T, I>>::InvalidJustification
		})?)
	}

	/// Import a previously verified header to the storage.
	///
	/// Note this function solely takes care of updating the storage and pruning old entries,
	/// but does not verify the validity of such import.
	pub(crate) fn insert_header<T: Config<I>, I: 'static>(
		header: BridgedHeader<T, I>,
		hash: BridgedBlockHash<T, I>,
	) {
		let index = <ImportedHashesPointer<T, I>>::get();
		let pruning = <ImportedHashes<T, I>>::try_get(index);
		<BestFinalized<T, I>>::put(HeaderId(*header.number(), hash));
		<ImportedHeaders<T, I>>::insert(hash, header.build());
		<ImportedHashes<T, I>>::insert(index, hash);

		// Update ring buffer pointer and remove old header.
		<ImportedHashesPointer<T, I>>::put((index + 1) % T::HeadersToKeep::get());
		if let Ok(hash) = pruning {
			log::debug!(target: LOG_TARGET, "Pruning old header: {:?}.", hash);
			<ImportedHeaders<T, I>>::remove(hash);
		}
	}

	/// Since this writes to storage with no real checks this should only be used in functions that
	/// were called by a trusted origin.
	pub(crate) fn initialize_bridge<T: Config<I>, I: 'static>(
		init_params: super::InitializationData<BridgedHeader<T, I>>,
	) -> Result<(), Error<T, I>> {
		let super::InitializationData { header, authority_list, set_id, operating_mode } =
			init_params;
		let authority_set_length = authority_list.len();
		let authority_set = StoredAuthoritySet::<T, I>::try_new(authority_list, set_id)
			.map_err(|e| {
				log::error!(
					target: LOG_TARGET,
					"Failed to initialize bridge. Number of authorities in the set {} is larger than the configured value {}",
					authority_set_length,
					T::BridgedChain::MAX_AUTHORITIES_COUNT,
				);

				e
			})?;
		let initial_hash = header.hash();

		<InitialHash<T, I>>::put(initial_hash);
		<ImportedHashesPointer<T, I>>::put(0);
		insert_header::<T, I>(*header, initial_hash);

		<CurrentAuthoritySet<T, I>>::put(authority_set);

		<PalletOperatingMode<T, I>>::put(operating_mode);

		Ok(())
	}

	/// Adapter for using `Config::HeadersToKeep` as `MaxValues` bound in our storage maps.
	pub struct MaybeHeadersToKeep<T, I>(PhantomData<(T, I)>);

	// this implementation is required to use the struct as `MaxValues`
	impl<T: Config<I>, I: 'static> Get<Option<u32>> for MaybeHeadersToKeep<T, I> {
		fn get() -> Option<u32> {
			Some(T::HeadersToKeep::get())
		}
	}

	/// Initialize pallet so that it is ready for inserting new header.
	///
	/// The function makes sure that the new insertion will cause the pruning of some old header.
	///
	/// Returns parent header for the new header.
	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn bootstrap_bridge<T: Config<I>, I: 'static>(
		init_params: super::InitializationData<BridgedHeader<T, I>>,
	) -> BridgedHeader<T, I> {
		let start_header = init_params.header.clone();
		initialize_bridge::<T, I>(init_params).expect("benchmarks are correct");

		// the most obvious way to cause pruning during next insertion would be to insert
		// `HeadersToKeep` headers. But it'll make our benchmarks slow. So we will just play with
		// our pruning ring-buffer.
		assert_eq!(ImportedHashesPointer::<T, I>::get(), 1);
		ImportedHashesPointer::<T, I>::put(0);

		*start_header
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I>
where
	<T as frame_system::Config>::RuntimeEvent: TryInto<Event<T, I>>,
{
	/// Get the GRANDPA justifications accepted in the current block.
	pub fn synced_headers_grandpa_info() -> Vec<StoredHeaderGrandpaInfo<BridgedHeader<T, I>>> {
		frame_system::Pallet::<T>::read_events_no_consensus()
			.filter_map(|event| {
				if let Event::<T, I>::UpdatedBestFinalizedHeader { grandpa_info, .. } =
					event.event.try_into().ok()?
				{
					return Some(grandpa_info)
				}
				None
			})
			.collect()
	}
}

/// Bridge GRANDPA pallet as header chain.
pub type GrandpaChainHeaders<T, I> = Pallet<T, I>;

impl<T: Config<I>, I: 'static> HeaderChain<BridgedChain<T, I>> for GrandpaChainHeaders<T, I> {
	fn finalized_header_state_root(
		header_hash: HashOf<BridgedChain<T, I>>,
	) -> Option<HashOf<BridgedChain<T, I>>> {
		ImportedHeaders::<T, I>::get(header_hash).map(|h| h.state_root)
	}
}

/// (Re)initialize bridge with given header for using it in `pallet-bridge-messages` benchmarks.
#[cfg(feature = "runtime-benchmarks")]
pub fn initialize_for_benchmarks<T: Config<I>, I: 'static>(header: BridgedHeader<T, I>) {
	initialize_bridge::<T, I>(InitializationData {
		header: Box::new(header),
		authority_list: sp_std::vec::Vec::new(), /* we don't verify any proofs in external
		                                          * benchmarks */
		set_id: 0,
		operating_mode: bp_runtime::BasicOperatingMode::Normal,
	})
	.expect("only used from benchmarks; benchmarks are correct; qed");
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{
		run_test, test_header, RuntimeEvent as TestEvent, RuntimeOrigin, System, TestBridgedChain,
		TestHeader, TestNumber, TestRuntime, MAX_BRIDGED_AUTHORITIES,
	};
	use bp_header_chain::BridgeGrandpaCall;
	use bp_runtime::BasicOperatingMode;
	use bp_test_utils::{
		authority_list, generate_owned_bridge_module_tests, make_default_justification,
		make_justification_for_header, JustificationGeneratorParams, ALICE, BOB,
		TEST_GRANDPA_SET_ID,
	};
	use codec::Encode;
	use frame_support::{
		assert_err, assert_noop, assert_ok,
		dispatch::{Pays, PostDispatchInfo},
		storage::generator::StorageValue,
	};
	use frame_system::{EventRecord, Phase};
	use sp_consensus_grandpa::{ConsensusLog, GRANDPA_ENGINE_ID};
	use sp_core::Get;
	use sp_runtime::{Digest, DigestItem, DispatchError};

	fn initialize_substrate_bridge() {
		System::set_block_number(1);
		System::reset_events();

		assert_ok!(init_with_origin(RuntimeOrigin::root()));
	}

	fn init_with_origin(
		origin: RuntimeOrigin,
	) -> Result<
		InitializationData<TestHeader>,
		sp_runtime::DispatchErrorWithPostInfo<PostDispatchInfo>,
	> {
		let genesis = test_header(0);

		let init_data = InitializationData {
			header: Box::new(genesis),
			authority_list: authority_list(),
			set_id: TEST_GRANDPA_SET_ID,
			operating_mode: BasicOperatingMode::Normal,
		};

		Pallet::<TestRuntime>::initialize(origin, init_data.clone()).map(|_| init_data)
	}

	fn submit_finality_proof(header: u8) -> frame_support::dispatch::DispatchResultWithPostInfo {
		let header = test_header(header.into());
		let justification = make_default_justification(&header);

		Pallet::<TestRuntime>::submit_finality_proof_ex(
			RuntimeOrigin::signed(1),
			Box::new(header),
			justification,
			TEST_GRANDPA_SET_ID,
		)
	}

	fn submit_finality_proof_with_set_id(
		header: u8,
		set_id: u64,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		let header = test_header(header.into());
		let justification = make_justification_for_header(JustificationGeneratorParams {
			header: header.clone(),
			set_id,
			..Default::default()
		});

		Pallet::<TestRuntime>::submit_finality_proof_ex(
			RuntimeOrigin::signed(1),
			Box::new(header),
			justification,
			set_id,
		)
	}

	fn submit_mandatory_finality_proof(
		number: u8,
		set_id: u64,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		let mut header = test_header(number.into());
		// to ease tests that are using `submit_mandatory_finality_proof`, we'll be using the
		// same set for all sessions
		let consensus_log =
			ConsensusLog::<TestNumber>::ScheduledChange(sp_consensus_grandpa::ScheduledChange {
				next_authorities: authority_list(),
				delay: 0,
			});
		header.digest =
			Digest { logs: vec![DigestItem::Consensus(GRANDPA_ENGINE_ID, consensus_log.encode())] };
		let justification = make_justification_for_header(JustificationGeneratorParams {
			header: header.clone(),
			set_id,
			..Default::default()
		});

		Pallet::<TestRuntime>::submit_finality_proof_ex(
			RuntimeOrigin::signed(1),
			Box::new(header),
			justification,
			set_id,
		)
	}

	fn next_block() {
		use frame_support::traits::OnInitialize;

		let current_number = frame_system::Pallet::<TestRuntime>::block_number();
		frame_system::Pallet::<TestRuntime>::set_block_number(current_number + 1);
		let _ = Pallet::<TestRuntime>::on_initialize(current_number);
	}

	fn change_log(delay: u64) -> Digest {
		let consensus_log =
			ConsensusLog::<TestNumber>::ScheduledChange(sp_consensus_grandpa::ScheduledChange {
				next_authorities: vec![(ALICE.into(), 1), (BOB.into(), 1)],
				delay,
			});

		Digest { logs: vec![DigestItem::Consensus(GRANDPA_ENGINE_ID, consensus_log.encode())] }
	}

	fn forced_change_log(delay: u64) -> Digest {
		let consensus_log = ConsensusLog::<TestNumber>::ForcedChange(
			delay,
			sp_consensus_grandpa::ScheduledChange {
				next_authorities: vec![(ALICE.into(), 1), (BOB.into(), 1)],
				delay,
			},
		);

		Digest { logs: vec![DigestItem::Consensus(GRANDPA_ENGINE_ID, consensus_log.encode())] }
	}

	fn many_authorities_log() -> Digest {
		let consensus_log =
			ConsensusLog::<TestNumber>::ScheduledChange(sp_consensus_grandpa::ScheduledChange {
				next_authorities: std::iter::repeat((ALICE.into(), 1))
					.take(MAX_BRIDGED_AUTHORITIES as usize + 1)
					.collect(),
				delay: 0,
			});

		Digest { logs: vec![DigestItem::Consensus(GRANDPA_ENGINE_ID, consensus_log.encode())] }
	}

	#[test]
	fn init_root_or_owner_origin_can_initialize_pallet() {
		run_test(|| {
			assert_noop!(init_with_origin(RuntimeOrigin::signed(1)), DispatchError::BadOrigin);
			assert_ok!(init_with_origin(RuntimeOrigin::root()));

			// Reset storage so we can initialize the pallet again
			BestFinalized::<TestRuntime>::kill();
			PalletOwner::<TestRuntime>::put(2);
			assert_ok!(init_with_origin(RuntimeOrigin::signed(2)));
		})
	}

	#[test]
	fn init_storage_entries_are_correctly_initialized() {
		run_test(|| {
			assert_eq!(BestFinalized::<TestRuntime>::get(), None,);
			assert_eq!(Pallet::<TestRuntime>::best_finalized(), None);
			assert_eq!(PalletOperatingMode::<TestRuntime>::try_get(), Err(()));

			let init_data = init_with_origin(RuntimeOrigin::root()).unwrap();

			assert!(<ImportedHeaders<TestRuntime>>::contains_key(init_data.header.hash()));
			assert_eq!(BestFinalized::<TestRuntime>::get().unwrap().1, init_data.header.hash());
			assert_eq!(
				CurrentAuthoritySet::<TestRuntime>::get().authorities,
				init_data.authority_list
			);
			assert_eq!(
				PalletOperatingMode::<TestRuntime>::try_get(),
				Ok(BasicOperatingMode::Normal)
			);
		})
	}

	#[test]
	fn init_can_only_initialize_pallet_once() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_noop!(
				init_with_origin(RuntimeOrigin::root()),
				<Error<TestRuntime>>::AlreadyInitialized
			);
		})
	}

	#[test]
	fn init_fails_if_there_are_too_many_authorities_in_the_set() {
		run_test(|| {
			let genesis = test_header(0);
			let init_data = InitializationData {
				header: Box::new(genesis),
				authority_list: std::iter::repeat(authority_list().remove(0))
					.take(MAX_BRIDGED_AUTHORITIES as usize + 1)
					.collect(),
				set_id: 1,
				operating_mode: BasicOperatingMode::Normal,
			};

			assert_noop!(
				Pallet::<TestRuntime>::initialize(RuntimeOrigin::root(), init_data),
				Error::<TestRuntime>::TooManyAuthoritiesInSet,
			);
		});
	}

	#[test]
	fn pallet_rejects_transactions_if_halted() {
		run_test(|| {
			initialize_substrate_bridge();

			assert_ok!(Pallet::<TestRuntime>::set_operating_mode(
				RuntimeOrigin::root(),
				BasicOperatingMode::Halted
			));
			assert_noop!(
				submit_finality_proof(1),
				Error::<TestRuntime>::BridgeModule(bp_runtime::OwnedBridgeModuleError::Halted)
			);

			assert_ok!(Pallet::<TestRuntime>::set_operating_mode(
				RuntimeOrigin::root(),
				BasicOperatingMode::Normal
			));
			assert_ok!(submit_finality_proof(1));
		})
	}

	#[test]
	fn pallet_rejects_header_if_not_initialized_yet() {
		run_test(|| {
			assert_noop!(submit_finality_proof(1), Error::<TestRuntime>::NotInitialized);
		});
	}

	#[test]
	fn successfully_imports_header_with_valid_finality() {
		run_test(|| {
			initialize_substrate_bridge();

			let header_number = 1;
			let header = test_header(header_number.into());
			let justification = make_default_justification(&header);

			let pre_dispatch_weight = <TestRuntime as Config>::WeightInfo::submit_finality_proof(
				justification.commit.precommits.len().try_into().unwrap_or(u32::MAX),
				justification.votes_ancestries.len().try_into().unwrap_or(u32::MAX),
			);

			let result = submit_finality_proof(header_number);
			assert_ok!(result);
			assert_eq!(result.unwrap().pays_fee, frame_support::dispatch::Pays::Yes);
			// our test config assumes 2048 max authorities and we are just using couple
			let pre_dispatch_proof_size = pre_dispatch_weight.proof_size();
			let actual_proof_size = result.unwrap().actual_weight.unwrap().proof_size();
			assert!(actual_proof_size > 0);
			assert!(
				actual_proof_size < pre_dispatch_proof_size,
				"Actual proof size {actual_proof_size} must be less than the pre-dispatch {pre_dispatch_proof_size}",
			);

			let header = test_header(1);
			assert_eq!(<BestFinalized<TestRuntime>>::get().unwrap().1, header.hash());
			assert!(<ImportedHeaders<TestRuntime>>::contains_key(header.hash()));

			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::Grandpa(Event::UpdatedBestFinalizedHeader {
						number: *header.number(),
						hash: header.hash(),
						grandpa_info: StoredHeaderGrandpaInfo {
							finality_proof: justification.clone(),
							new_verification_context: None,
						},
					}),
					topics: vec![],
				}],
			);
			assert_eq!(
				Pallet::<TestRuntime>::synced_headers_grandpa_info(),
				vec![StoredHeaderGrandpaInfo {
					finality_proof: justification,
					new_verification_context: None
				}]
			);
		})
	}

	#[test]
	fn rejects_justification_that_skips_authority_set_transition() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(1);

			let next_set_id = 2;
			let params = JustificationGeneratorParams::<TestHeader> {
				set_id: next_set_id,
				..Default::default()
			};
			let justification = make_justification_for_header(params);

			assert_err!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header.clone()),
					justification.clone(),
					TEST_GRANDPA_SET_ID,
				),
				<Error<TestRuntime>>::InvalidJustification
			);
			assert_err!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header),
					justification,
					next_set_id,
				),
				<Error<TestRuntime>>::InvalidAuthoritySetId
			);
		})
	}

	#[test]
	fn does_not_import_header_with_invalid_finality_proof() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(1);
			let mut justification = make_default_justification(&header);
			justification.round = 42;

			assert_err!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header),
					justification,
					TEST_GRANDPA_SET_ID,
				),
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
				header: Box::new(genesis),
				authority_list: invalid_authority_list,
				set_id: 1,
				operating_mode: BasicOperatingMode::Normal,
			};

			assert_ok!(Pallet::<TestRuntime>::initialize(RuntimeOrigin::root(), init_data));

			let header = test_header(1);
			let justification = make_default_justification(&header);

			assert_err!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header),
					justification,
					TEST_GRANDPA_SET_ID,
				),
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
			let justification = make_default_justification(&header);

			// Let's import our test header
			let result = Pallet::<TestRuntime>::submit_finality_proof_ex(
				RuntimeOrigin::signed(1),
				Box::new(header.clone()),
				justification.clone(),
				TEST_GRANDPA_SET_ID,
			);
			assert_ok!(result);
			assert_eq!(result.unwrap().pays_fee, frame_support::dispatch::Pays::No);

			// Make sure that our header is the best finalized
			assert_eq!(<BestFinalized<TestRuntime>>::get().unwrap().1, header.hash());
			assert!(<ImportedHeaders<TestRuntime>>::contains_key(header.hash()));

			// Make sure that the authority set actually changed upon importing our header
			assert_eq!(
				<CurrentAuthoritySet<TestRuntime>>::get(),
				StoredAuthoritySet::<TestRuntime, ()>::try_new(next_authorities, next_set_id)
					.unwrap(),
			);

			// Here
			assert_eq!(
				System::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::Grandpa(Event::UpdatedBestFinalizedHeader {
						number: *header.number(),
						hash: header.hash(),
						grandpa_info: StoredHeaderGrandpaInfo {
							finality_proof: justification.clone(),
							new_verification_context: Some(
								<CurrentAuthoritySet<TestRuntime>>::get().into()
							),
						},
					}),
					topics: vec![],
				}],
			);
			assert_eq!(
				Pallet::<TestRuntime>::synced_headers_grandpa_info(),
				vec![StoredHeaderGrandpaInfo {
					finality_proof: justification,
					new_verification_context: Some(
						<CurrentAuthoritySet<TestRuntime>>::get().into()
					),
				}]
			);
		})
	}

	#[test]
	fn relayer_pays_tx_fee_when_submitting_huge_mandatory_header() {
		run_test(|| {
			initialize_substrate_bridge();

			// let's prepare a huge authorities change header, which is definitely above size limits
			let mut header = test_header(2);
			header.digest = change_log(0);
			header.digest.push(DigestItem::Other(vec![42u8; 1024 * 1024]));
			let justification = make_default_justification(&header);

			// without large digest item ^^^ the relayer would have paid zero transaction fee
			// (`Pays::No`)
			let result = Pallet::<TestRuntime>::submit_finality_proof_ex(
				RuntimeOrigin::signed(1),
				Box::new(header.clone()),
				justification,
				TEST_GRANDPA_SET_ID,
			);
			assert_ok!(result);
			assert_eq!(result.unwrap().pays_fee, frame_support::dispatch::Pays::Yes);

			// Make sure that our header is the best finalized
			assert_eq!(<BestFinalized<TestRuntime>>::get().unwrap().1, header.hash());
			assert!(<ImportedHeaders<TestRuntime>>::contains_key(header.hash()));
		})
	}

	#[test]
	fn relayer_pays_tx_fee_when_submitting_justification_with_long_ancestry_votes() {
		run_test(|| {
			initialize_substrate_bridge();

			// let's prepare a huge authorities change header, which is definitely above weight
			// limits
			let mut header = test_header(2);
			header.digest = change_log(0);
			let justification = make_justification_for_header(JustificationGeneratorParams {
				header: header.clone(),
				ancestors: TestBridgedChain::REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY + 1,
				..Default::default()
			});

			// without many headers in votes ancestries ^^^ the relayer would have paid zero
			// transaction fee (`Pays::No`)
			let result = Pallet::<TestRuntime>::submit_finality_proof_ex(
				RuntimeOrigin::signed(1),
				Box::new(header.clone()),
				justification,
				TEST_GRANDPA_SET_ID,
			);
			assert_ok!(result);
			assert_eq!(result.unwrap().pays_fee, frame_support::dispatch::Pays::Yes);

			// Make sure that our header is the best finalized
			assert_eq!(<BestFinalized<TestRuntime>>::get().unwrap().1, header.hash());
			assert!(<ImportedHeaders<TestRuntime>>::contains_key(header.hash()));
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
			let justification = make_default_justification(&header);

			// Should not be allowed to import this header
			assert_err!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header),
					justification,
					TEST_GRANDPA_SET_ID,
				),
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
			let justification = make_default_justification(&header);

			// Should not be allowed to import this header
			assert_err!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header),
					justification,
					TEST_GRANDPA_SET_ID,
				),
				<Error<TestRuntime>>::UnsupportedScheduledChange
			);
		})
	}

	#[test]
	fn importing_header_rejects_header_with_too_many_authorities() {
		run_test(|| {
			initialize_substrate_bridge();

			// Need to update the header digest to indicate that our header signals an authority set
			// change. However, the change doesn't happen until the next block.
			let mut header = test_header(2);
			header.digest = many_authorities_log();

			// Create a valid justification for the header
			let justification = make_default_justification(&header);

			// Should not be allowed to import this header
			assert_err!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header),
					justification,
					TEST_GRANDPA_SET_ID,
				),
				<Error<TestRuntime>>::TooManyAuthoritiesInSet
			);
		});
	}

	#[test]
	fn parse_finalized_storage_proof_rejects_proof_on_unknown_header() {
		run_test(|| {
			assert_noop!(
				Pallet::<TestRuntime>::storage_proof_checker(Default::default(), vec![],)
					.map(|_| ()),
				bp_header_chain::HeaderChainError::UnknownHeader,
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
			<BestFinalized<TestRuntime>>::put(HeaderId(2, hash));
			<ImportedHeaders<TestRuntime>>::insert(hash, header.build());

			assert_ok!(
				Pallet::<TestRuntime>::storage_proof_checker(hash, storage_proof).map(|_| ())
			);
		});
	}

	#[test]
	fn rate_limiter_disallows_free_imports_once_limit_is_hit_in_single_block() {
		run_test(|| {
			initialize_substrate_bridge();

			let result = submit_mandatory_finality_proof(1, 1);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(2, 2);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(3, 3);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);
		})
	}

	#[test]
	fn rate_limiter_invalid_requests_do_not_count_towards_request_count() {
		run_test(|| {
			let submit_invalid_request = || {
				let mut header = test_header(1);
				header.digest = change_log(0);
				let mut invalid_justification = make_default_justification(&header);
				invalid_justification.round = 42;

				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::signed(1),
					Box::new(header),
					invalid_justification,
					TEST_GRANDPA_SET_ID,
				)
			};

			initialize_substrate_bridge();

			for _ in 0..<TestRuntime as Config>::MaxFreeMandatoryHeadersPerBlock::get() + 1 {
				assert_err!(submit_invalid_request(), <Error<TestRuntime>>::InvalidJustification);
			}

			// Can still submit free mandatory headers afterwards
			let result = submit_mandatory_finality_proof(1, 1);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(2, 2);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(3, 3);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);
		})
	}

	#[test]
	fn rate_limiter_allows_request_after_new_block_has_started() {
		run_test(|| {
			initialize_substrate_bridge();

			let result = submit_mandatory_finality_proof(1, 1);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(2, 2);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(3, 3);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);

			next_block();

			let result = submit_mandatory_finality_proof(4, 4);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(5, 5);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_mandatory_finality_proof(6, 6);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);
		})
	}

	#[test]
	fn rate_limiter_ignores_non_mandatory_headers() {
		run_test(|| {
			initialize_substrate_bridge();

			let result = submit_finality_proof(1);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);

			let result = submit_mandatory_finality_proof(2, 1);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_finality_proof_with_set_id(3, 2);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);

			let result = submit_mandatory_finality_proof(4, 2);
			assert_eq!(result.expect("call failed").pays_fee, Pays::No);

			let result = submit_finality_proof_with_set_id(5, 3);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);

			let result = submit_mandatory_finality_proof(6, 3);
			assert_eq!(result.expect("call failed").pays_fee, Pays::Yes);
		})
	}

	#[test]
	fn should_prune_headers_over_headers_to_keep_parameter() {
		run_test(|| {
			initialize_substrate_bridge();
			assert_ok!(submit_finality_proof(1));
			let first_header_hash = Pallet::<TestRuntime>::best_finalized().unwrap().hash();
			next_block();

			assert_ok!(submit_finality_proof(2));
			next_block();
			assert_ok!(submit_finality_proof(3));
			next_block();
			assert_ok!(submit_finality_proof(4));
			next_block();
			assert_ok!(submit_finality_proof(5));
			next_block();

			assert_ok!(submit_finality_proof(6));

			assert!(
				!ImportedHeaders::<TestRuntime, ()>::contains_key(first_header_hash),
				"First header should be pruned.",
			);
		})
	}

	#[test]
	fn storage_keys_computed_properly() {
		assert_eq!(
			PalletOperatingMode::<TestRuntime>::storage_value_final_key().to_vec(),
			bp_header_chain::storage_keys::pallet_operating_mode_key("Grandpa").0,
		);

		assert_eq!(
			CurrentAuthoritySet::<TestRuntime>::storage_value_final_key().to_vec(),
			bp_header_chain::storage_keys::current_authority_set_key("Grandpa").0,
		);

		assert_eq!(
			BestFinalized::<TestRuntime>::storage_value_final_key().to_vec(),
			bp_header_chain::storage_keys::best_finalized_key("Grandpa").0,
		);
	}

	#[test]
	fn test_bridge_grandpa_call_is_correctly_defined() {
		let header = test_header(0);
		let init_data = InitializationData {
			header: Box::new(header.clone()),
			authority_list: authority_list(),
			set_id: 1,
			operating_mode: BasicOperatingMode::Normal,
		};
		let justification = make_default_justification(&header);

		let direct_initialize_call =
			Call::<TestRuntime>::initialize { init_data: init_data.clone() };
		let indirect_initialize_call = BridgeGrandpaCall::<TestHeader>::initialize { init_data };
		assert_eq!(direct_initialize_call.encode(), indirect_initialize_call.encode());

		let direct_submit_finality_proof_call = Call::<TestRuntime>::submit_finality_proof {
			finality_target: Box::new(header.clone()),
			justification: justification.clone(),
		};
		let indirect_submit_finality_proof_call =
			BridgeGrandpaCall::<TestHeader>::submit_finality_proof {
				finality_target: Box::new(header),
				justification,
			};
		assert_eq!(
			direct_submit_finality_proof_call.encode(),
			indirect_submit_finality_proof_call.encode()
		);
	}

	generate_owned_bridge_module_tests!(BasicOperatingMode::Normal, BasicOperatingMode::Halted);

	#[test]
	fn maybe_headers_to_keep_returns_correct_value() {
		assert_eq!(MaybeHeadersToKeep::<TestRuntime, ()>::get(), Some(mock::HeadersToKeep::get()));
	}

	#[test]
	fn submit_finality_proof_requires_signed_origin() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(1);
			let justification = make_default_justification(&header);

			assert_noop!(
				Pallet::<TestRuntime>::submit_finality_proof_ex(
					RuntimeOrigin::root(),
					Box::new(header),
					justification,
					TEST_GRANDPA_SET_ID,
				),
				DispatchError::BadOrigin,
			);
		})
	}
}
