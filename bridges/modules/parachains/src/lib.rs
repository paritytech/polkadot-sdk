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

//! Parachains finality module.
//!
//! This module needs to be deployed with GRANDPA module, which is syncing relay
//! chain blocks. The main entry point of this module is `submit_parachain_heads`, which
//! accepts storage proof of some parachain `Heads` entries from bridged relay chain.
//! It requires corresponding relay headers to be already synced.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

pub use weights::WeightInfo;
pub use weights_ext::WeightInfoExt;

use bp_header_chain::{HeaderChain, HeaderChainError};
use bp_parachains::{parachain_head_storage_key_at_source, ParaInfo, ParaStoredHeaderData};
use bp_polkadot_core::parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId};
use bp_runtime::{Chain, HashOf, HeaderId, HeaderIdOf, Parachain, StorageProofError};
use frame_support::{dispatch::PostDispatchInfo, DefaultNoBound};
use sp_std::{marker::PhantomData, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
use bp_parachains::ParaStoredHeaderDataBuilder;
#[cfg(feature = "runtime-benchmarks")]
use bp_runtime::HeaderOf;
#[cfg(feature = "runtime-benchmarks")]
use codec::Encode;

// Re-export in crate namespace for `construct_runtime!`.
pub use call_ext::*;
pub use pallet::*;

pub mod weights;
pub mod weights_ext;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

mod call_ext;
#[cfg(test)]
mod mock;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "runtime::bridge-parachains";

/// Block hash of the bridged relay chain.
pub type RelayBlockHash = bp_polkadot_core::Hash;
/// Block number of the bridged relay chain.
pub type RelayBlockNumber = bp_polkadot_core::BlockNumber;
/// Hasher of the bridged relay chain.
pub type RelayBlockHasher = bp_polkadot_core::Hasher;

/// Artifacts of the parachains head update.
struct UpdateParachainHeadArtifacts {
	/// New best head of the parachain.
	pub best_head: ParaInfo,
	/// If `true`, some old parachain head has been pruned during update.
	pub prune_happened: bool,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use bp_parachains::{
		BestParaHeadHash, ImportedParaHeadsKeyProvider, ParaStoredHeaderDataBuilder,
		ParasInfoKeyProvider,
	};
	use bp_runtime::{
		BasicOperatingMode, BoundedStorageValue, OwnedBridgeModule, StorageDoubleMapKeyProvider,
		StorageMapKeyProvider,
	};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Stored parachain head data of given parachains pallet.
	pub type StoredParaHeadDataOf<T, I> =
		BoundedStorageValue<<T as Config<I>>::MaxParaHeadDataSize, ParaStoredHeaderData>;
	/// Weight info of the given parachains pallet.
	pub type WeightInfoOf<T, I> = <T as Config<I>>::WeightInfo;
	type GrandpaPalletOf<T, I> =
		pallet_bridge_grandpa::Pallet<T, <T as Config<I>>::BridgesGrandpaPalletInstance>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The caller has provided head of parachain that the pallet is not configured to track.
		UntrackedParachainRejected {
			/// Identifier of the parachain that is not tracked by the pallet.
			parachain: ParaId,
		},
		/// The caller has declared that he has provided given parachain head, but it is missing
		/// from the storage proof.
		MissingParachainHead {
			/// Identifier of the parachain with missing head.
			parachain: ParaId,
		},
		/// The caller has provided parachain head hash that is not matching the hash read from the
		/// storage proof.
		IncorrectParachainHeadHash {
			/// Identifier of the parachain with incorrect head hast.
			parachain: ParaId,
			/// Specified parachain head hash.
			parachain_head_hash: ParaHash,
			/// Actual parachain head hash.
			actual_parachain_head_hash: ParaHash,
		},
		/// The caller has provided obsolete parachain head, which is already known to the pallet.
		RejectedObsoleteParachainHead {
			/// Identifier of the parachain with obsolete head.
			parachain: ParaId,
			/// Obsolete parachain head hash.
			parachain_head_hash: ParaHash,
		},
		/// The caller has provided parachain head that exceeds the maximal configured head size.
		RejectedLargeParachainHead {
			/// Identifier of the parachain with rejected head.
			parachain: ParaId,
			/// Parachain head hash.
			parachain_head_hash: ParaHash,
			/// Parachain head size.
			parachain_head_size: u32,
		},
		/// Parachain head has been updated.
		UpdatedParachainHead {
			/// Identifier of the parachain that has been updated.
			parachain: ParaId,
			/// Parachain head hash.
			parachain_head_hash: ParaHash,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Relay chain block hash is unknown to us.
		UnknownRelayChainBlock,
		/// The number of stored relay block is different from what the relayer has provided.
		InvalidRelayChainBlockNumber,
		/// Parachain heads storage proof is invalid.
		HeaderChainStorageProof(HeaderChainError),
		/// Error generated by the `OwnedBridgeModule` trait.
		BridgeModule(bp_runtime::OwnedBridgeModuleError),
	}

	/// Convenience trait for defining `BridgedChain` bounds.
	pub trait BoundedBridgeGrandpaConfig<I: 'static>:
		pallet_bridge_grandpa::Config<I, BridgedChain = Self::BridgedRelayChain>
	{
		/// Type of the bridged relay chain.
		type BridgedRelayChain: Chain<
			BlockNumber = RelayBlockNumber,
			Hash = RelayBlockHash,
			Hasher = RelayBlockHasher,
		>;
	}

	impl<T, I: 'static> BoundedBridgeGrandpaConfig<I> for T
	where
		T: pallet_bridge_grandpa::Config<I>,
		T::BridgedChain:
			Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>,
	{
		type BridgedRelayChain = T::BridgedChain;
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>:
		BoundedBridgeGrandpaConfig<Self::BridgesGrandpaPalletInstance>
	{
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Benchmarks results from runtime we're plugged into.
		type WeightInfo: WeightInfoExt;

		/// Instance of bridges GRANDPA pallet (within this runtime) that this pallet is linked to.
		///
		/// The GRANDPA pallet instance must be configured to import headers of relay chain that
		/// we're interested in.
		type BridgesGrandpaPalletInstance: 'static;

		/// Name of the original `paras` pallet in the `construct_runtime!()` call at the bridged
		/// chain.
		///
		/// Please keep in mind that this should be the name of the `runtime_parachains::paras`
		/// pallet from polkadot repository, not the `pallet-bridge-parachains`.
		#[pallet::constant]
		type ParasPalletName: Get<&'static str>;

		/// Parachain head data builder.
		///
		/// We never store parachain heads here, since they may be too big (e.g. because of large
		/// digest items). Instead we're using the same approach as `pallet-bridge-grandpa`
		/// pallet - we are only storing `bp_messages::StoredHeaderData` (number and state root),
		/// which is enough for our applications. However, we work with different parachains here
		/// and they can use different primitives (for block numbers and hash). So we can't store
		/// it directly. Instead, we're storing `bp_messages::StoredHeaderData` in SCALE-encoded
		/// form, wrapping it into `bp_parachains::ParaStoredHeaderData`.
		///
		/// This builder helps to convert from `HeadData` to `bp_parachains::ParaStoredHeaderData`.
		type ParaStoredHeaderDataBuilder: ParaStoredHeaderDataBuilder;

		/// Maximal number of single parachain heads to keep in the storage.
		///
		/// The setting is there to prevent growing the on-chain state indefinitely. Note
		/// the setting does not relate to parachain block numbers - we will simply keep as much
		/// items in the storage, so it doesn't guarantee any fixed timeframe for heads.
		///
		/// Incautious change of this constant may lead to orphan entries in the runtime storage.
		#[pallet::constant]
		type HeadsToKeep: Get<u32>;

		/// Maximal size (in bytes) of the SCALE-encoded parachain head data
		/// (`bp_parachains::ParaStoredHeaderData`).
		///
		/// Keep in mind that the size of any tracked parachain header data must not exceed this
		/// value. So if you're going to track multiple parachains, one of which is using large
		/// hashes, you shall choose this maximal value.
		///
		/// There's no mandatory headers in this pallet, so it can't stall if there's some header
		/// that exceeds this bound.
		#[pallet::constant]
		type MaxParaHeadDataSize: Get<u32>;
	}

	/// Optional pallet owner.
	///
	/// Pallet owner has a right to halt all pallet operations and then resume them. If it is
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

	/// Parachains info.
	///
	/// Contains the following info:
	/// - best parachain head hash
	/// - the head of the `ImportedParaHashes` ring buffer
	#[pallet::storage]
	pub type ParasInfo<T: Config<I>, I: 'static = ()> = StorageMap<
		Hasher = <ParasInfoKeyProvider as StorageMapKeyProvider>::Hasher,
		Key = <ParasInfoKeyProvider as StorageMapKeyProvider>::Key,
		Value = <ParasInfoKeyProvider as StorageMapKeyProvider>::Value,
		QueryKind = OptionQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeMaxParachains<T, I>,
	>;

	/// State roots of parachain heads which have been imported into the pallet.
	#[pallet::storage]
	pub type ImportedParaHeads<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		Hasher1 = <ImportedParaHeadsKeyProvider as StorageDoubleMapKeyProvider>::Hasher1,
		Key1 = <ImportedParaHeadsKeyProvider as StorageDoubleMapKeyProvider>::Key1,
		Hasher2 = <ImportedParaHeadsKeyProvider as StorageDoubleMapKeyProvider>::Hasher2,
		Key2 = <ImportedParaHeadsKeyProvider as StorageDoubleMapKeyProvider>::Key2,
		Value = StoredParaHeadDataOf<T, I>,
		QueryKind = OptionQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeMaxTotalParachainHashes<T, I>,
	>;

	/// A ring buffer of imported parachain head hashes. Ordered by the insertion time.
	#[pallet::storage]
	pub(super) type ImportedParaHashes<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		Hasher1 = Blake2_128Concat,
		Key1 = ParaId,
		Hasher2 = Twox64Concat,
		Key2 = u32,
		Value = ParaHash,
		QueryKind = OptionQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeMaxTotalParachainHashes<T, I>,
	>;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> OwnedBridgeModule<T> for Pallet<T, I> {
		const LOG_TARGET: &'static str = LOG_TARGET;
		type OwnerStorage = PalletOwner<T, I>;
		type OperatingMode = BasicOperatingMode;
		type OperatingModeStorage = PalletOperatingMode<T, I>;
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Submit proof of one or several parachain heads.
		///
		/// The proof is supposed to be proof of some `Heads` entries from the
		/// `polkadot-runtime-parachains::paras` pallet instance, deployed at the bridged chain.
		/// The proof is supposed to be crafted at the `relay_header_hash` that must already be
		/// imported by corresponding GRANDPA pallet at this chain.
		///
		/// The call fails if:
		///
		/// - the pallet is halted;
		///
		/// - the relay chain block `at_relay_block` is not imported by the associated bridge
		///   GRANDPA pallet.
		///
		/// The call may succeed, but some heads may not be updated e.g. because pallet knows
		/// better head or it isn't tracked by the pallet.
		#[pallet::call_index(0)]
		#[pallet::weight(WeightInfoOf::<T, I>::submit_parachain_heads_weight(
			T::DbWeight::get(),
			parachain_heads_proof,
			parachains.len() as _,
		))]
		pub fn submit_parachain_heads(
			origin: OriginFor<T>,
			at_relay_block: (RelayBlockNumber, RelayBlockHash),
			parachains: Vec<(ParaId, ParaHash)>,
			parachain_heads_proof: ParaHeadsProof,
		) -> DispatchResultWithPostInfo {
			Self::ensure_not_halted().map_err(Error::<T, I>::BridgeModule)?;
			ensure_signed(origin)?;

			// we'll need relay chain header to verify that parachains heads are always increasing.
			let (relay_block_number, relay_block_hash) = at_relay_block;
			let relay_block = pallet_bridge_grandpa::ImportedHeaders::<
				T,
				T::BridgesGrandpaPalletInstance,
			>::get(relay_block_hash)
			.ok_or(Error::<T, I>::UnknownRelayChainBlock)?;
			ensure!(
				relay_block.number == relay_block_number,
				Error::<T, I>::InvalidRelayChainBlockNumber,
			);

			// now parse storage proof and read parachain heads
			let mut actual_weight = WeightInfoOf::<T, I>::submit_parachain_heads_weight(
				T::DbWeight::get(),
				&parachain_heads_proof,
				parachains.len() as _,
			);

			let mut storage = GrandpaPalletOf::<T, I>::storage_proof_checker(
				relay_block_hash,
				parachain_heads_proof.storage_proof,
			)
			.map_err(Error::<T, I>::HeaderChainStorageProof)?;

			for (parachain, parachain_head_hash) in parachains {
				let parachain_head = match Self::read_parachain_head(&mut storage, parachain) {
					Ok(Some(parachain_head)) => parachain_head,
					Ok(None) => {
						log::trace!(
							target: LOG_TARGET,
							"The head of parachain {:?} is None. {}",
							parachain,
							if ParasInfo::<T, I>::contains_key(parachain) {
								"Looks like it is not yet registered at the source relay chain"
							} else {
								"Looks like it has been deregistered from the source relay chain"
							},
						);
						Self::deposit_event(Event::MissingParachainHead { parachain });
						continue
					},
					Err(e) => {
						log::trace!(
							target: LOG_TARGET,
							"The read of head of parachain {:?} has failed: {:?}",
							parachain,
							e,
						);
						Self::deposit_event(Event::MissingParachainHead { parachain });
						continue
					},
				};

				// if relayer has specified invalid parachain head hash, ignore the head
				// (this isn't strictly necessary, but better safe than sorry)
				let actual_parachain_head_hash = parachain_head.hash();
				if parachain_head_hash != actual_parachain_head_hash {
					log::trace!(
						target: LOG_TARGET,
						"The submitter has specified invalid parachain {:?} head hash: \
								{:?} vs {:?}",
						parachain,
						parachain_head_hash,
						actual_parachain_head_hash,
					);
					Self::deposit_event(Event::IncorrectParachainHeadHash {
						parachain,
						parachain_head_hash,
						actual_parachain_head_hash,
					});
					continue
				}

				// convert from parachain head into stored parachain head data
				let parachain_head_data =
					match T::ParaStoredHeaderDataBuilder::try_build(parachain, &parachain_head) {
						Some(parachain_head_data) => parachain_head_data,
						None => {
							log::trace!(
								target: LOG_TARGET,
								"The head of parachain {:?} has been provided, but it is not tracked by the pallet",
								parachain,
							);
							Self::deposit_event(Event::UntrackedParachainRejected { parachain });
							continue
						},
					};

				let update_result: Result<_, ()> =
					ParasInfo::<T, I>::try_mutate(parachain, |stored_best_head| {
						let artifacts = Pallet::<T, I>::update_parachain_head(
							parachain,
							stored_best_head.take(),
							relay_block_number,
							parachain_head_data,
							parachain_head_hash,
						)?;
						*stored_best_head = Some(artifacts.best_head);
						Ok(artifacts.prune_happened)
					});

				// we're refunding weight if update has not happened and if pruning has not happened
				let is_update_happened = update_result.is_ok();
				if !is_update_happened {
					actual_weight = actual_weight.saturating_sub(
						WeightInfoOf::<T, I>::parachain_head_storage_write_weight(
							T::DbWeight::get(),
						),
					);
				}
				let is_prune_happened = matches!(update_result, Ok(true));
				if !is_prune_happened {
					actual_weight = actual_weight.saturating_sub(
						WeightInfoOf::<T, I>::parachain_head_pruning_weight(T::DbWeight::get()),
					);
				}
			}

			// even though we may have accepted some parachain heads, we can't allow relayers to
			// submit proof with unused trie nodes
			// => treat this as an error
			//
			// (we can throw error here, because now all our calls are transactional)
			storage.ensure_no_unused_nodes().map_err(|e| {
				Error::<T, I>::HeaderChainStorageProof(HeaderChainError::StorageProof(e))
			})?;

			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::Yes })
		}

		/// Change `PalletOwner`.
		///
		/// May only be called either by root, or by `PalletOwner`.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_owner(origin: OriginFor<T>, new_owner: Option<T::AccountId>) -> DispatchResult {
			<Self as OwnedBridgeModule<_>>::set_owner(origin, new_owner)
		}

		/// Halt or resume all pallet operations.
		///
		/// May only be called either by root, or by `PalletOwner`.
		#[pallet::call_index(2)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operating_mode(
			origin: OriginFor<T>,
			operating_mode: BasicOperatingMode,
		) -> DispatchResult {
			<Self as OwnedBridgeModule<_>>::set_operating_mode(origin, operating_mode)
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Get stored parachain info.
		pub fn best_parachain_info(parachain: ParaId) -> Option<ParaInfo> {
			ParasInfo::<T, I>::get(parachain)
		}

		/// Get best finalized head data of the given parachain.
		pub fn best_parachain_head(parachain: ParaId) -> Option<ParaStoredHeaderData> {
			let best_para_head_hash = ParasInfo::<T, I>::get(parachain)?.best_head_hash.head_hash;
			ImportedParaHeads::<T, I>::get(parachain, best_para_head_hash).map(|h| h.into_inner())
		}

		/// Get best finalized head hash of the given parachain.
		pub fn best_parachain_head_hash(parachain: ParaId) -> Option<ParaHash> {
			Some(ParasInfo::<T, I>::get(parachain)?.best_head_hash.head_hash)
		}

		/// Get best finalized head id of the given parachain.
		pub fn best_parachain_head_id<C: Chain<Hash = ParaHash> + Parachain>(
		) -> Result<Option<HeaderIdOf<C>>, codec::Error> {
			let parachain = ParaId(C::PARACHAIN_ID);
			let best_head_hash = match Self::best_parachain_head_hash(parachain) {
				Some(best_head_hash) => best_head_hash,
				None => return Ok(None),
			};
			let encoded_head = match Self::parachain_head(parachain, best_head_hash) {
				Some(encoded_head) => encoded_head,
				None => return Ok(None),
			};
			encoded_head
				.decode_parachain_head_data::<C>()
				.map(|data| Some(HeaderId(data.number, best_head_hash)))
		}

		/// Get parachain head data with given hash.
		pub fn parachain_head(parachain: ParaId, hash: ParaHash) -> Option<ParaStoredHeaderData> {
			ImportedParaHeads::<T, I>::get(parachain, hash).map(|h| h.into_inner())
		}

		/// Read parachain head from storage proof.
		fn read_parachain_head(
			storage: &mut bp_runtime::StorageProofChecker<RelayBlockHasher>,
			parachain: ParaId,
		) -> Result<Option<ParaHead>, StorageProofError> {
			let parachain_head_key =
				parachain_head_storage_key_at_source(T::ParasPalletName::get(), parachain);
			storage.read_and_decode_value(parachain_head_key.0.as_ref())
		}

		/// Try to update parachain head.
		pub(super) fn update_parachain_head(
			parachain: ParaId,
			stored_best_head: Option<ParaInfo>,
			new_at_relay_block_number: RelayBlockNumber,
			new_head_data: ParaStoredHeaderData,
			new_head_hash: ParaHash,
		) -> Result<UpdateParachainHeadArtifacts, ()> {
			// check if head has been already updated at better relay chain block. Without this
			// check, we may import heads in random order
			let update = SubmitParachainHeadsInfo {
				at_relay_block_number: new_at_relay_block_number,
				para_id: parachain,
				para_head_hash: new_head_hash,
			};
			if SubmitParachainHeadsHelper::<T, I>::is_obsolete(&update) {
				Self::deposit_event(Event::RejectedObsoleteParachainHead {
					parachain,
					parachain_head_hash: new_head_hash,
				});
				return Err(())
			}

			// verify that the parachain head data size is <= `MaxParaHeadDataSize`
			let updated_head_data =
				match StoredParaHeadDataOf::<T, I>::try_from_inner(new_head_data) {
					Ok(updated_head_data) => updated_head_data,
					Err(e) => {
						log::trace!(
							target: LOG_TARGET,
							"The parachain head can't be updated. The parachain head data size \
							for {:?} is {}. It exceeds maximal configured size {}.",
							parachain,
							e.value_size,
							e.maximal_size,
						);

						Self::deposit_event(Event::RejectedLargeParachainHead {
							parachain,
							parachain_head_hash: new_head_hash,
							parachain_head_size: e.value_size as _,
						});

						return Err(())
					},
				};

			let next_imported_hash_position = stored_best_head
				.map_or(0, |stored_best_head| stored_best_head.next_imported_hash_position);

			// insert updated best parachain head
			let head_hash_to_prune =
				ImportedParaHashes::<T, I>::try_get(parachain, next_imported_hash_position);
			let updated_best_para_head = ParaInfo {
				best_head_hash: BestParaHeadHash {
					at_relay_block_number: new_at_relay_block_number,
					head_hash: new_head_hash,
				},
				next_imported_hash_position: (next_imported_hash_position + 1) %
					T::HeadsToKeep::get(),
			};
			ImportedParaHashes::<T, I>::insert(
				parachain,
				next_imported_hash_position,
				new_head_hash,
			);
			ImportedParaHeads::<T, I>::insert(parachain, new_head_hash, updated_head_data);
			log::trace!(
				target: LOG_TARGET,
				"Updated head of parachain {:?} to {}",
				parachain,
				new_head_hash,
			);

			// remove old head
			let prune_happened = head_hash_to_prune.is_ok();
			if let Ok(head_hash_to_prune) = head_hash_to_prune {
				log::trace!(
					target: LOG_TARGET,
					"Pruning old head of parachain {:?}: {}",
					parachain,
					head_hash_to_prune,
				);
				ImportedParaHeads::<T, I>::remove(parachain, head_hash_to_prune);
			}
			Self::deposit_event(Event::UpdatedParachainHead {
				parachain,
				parachain_head_hash: new_head_hash,
			});

			Ok(UpdateParachainHeadArtifacts { best_head: updated_best_para_head, prune_happened })
		}
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		/// Initial pallet operating mode.
		pub operating_mode: BasicOperatingMode,
		/// Initial pallet owner.
		pub owner: Option<T::AccountId>,
		/// Dummy marker.
		pub phantom: sp_std::marker::PhantomData<I>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			PalletOperatingMode::<T, I>::put(self.operating_mode);
			if let Some(ref owner) = self.owner {
				PalletOwner::<T, I>::put(owner);
			}
		}
	}

	/// Returns maximal number of parachains, supported by the pallet.
	pub struct MaybeMaxParachains<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> Get<Option<u32>> for MaybeMaxParachains<T, I> {
		fn get() -> Option<u32> {
			Some(T::ParaStoredHeaderDataBuilder::supported_parachains())
		}
	}

	/// Returns total number of all parachains hashes/heads, stored by the pallet.
	pub struct MaybeMaxTotalParachainHashes<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> Get<Option<u32>> for MaybeMaxTotalParachainHashes<T, I> {
		fn get() -> Option<u32> {
			Some(
				T::ParaStoredHeaderDataBuilder::supported_parachains()
					.saturating_mul(T::HeadsToKeep::get()),
			)
		}
	}
}

/// Single parachain header chain adapter.
pub struct ParachainHeaders<T, I, C>(PhantomData<(T, I, C)>);

impl<T: Config<I>, I: 'static, C: Parachain<Hash = ParaHash>> HeaderChain<C>
	for ParachainHeaders<T, I, C>
{
	fn finalized_header_state_root(hash: HashOf<C>) -> Option<HashOf<C>> {
		Pallet::<T, I>::parachain_head(ParaId(C::PARACHAIN_ID), hash)
			.and_then(|head| head.decode_parachain_head_data::<C>().ok())
			.map(|h| h.state_root)
	}
}

/// (Re)initialize pallet with given header for using it in `pallet-bridge-messages` benchmarks.
#[cfg(feature = "runtime-benchmarks")]
pub fn initialize_for_benchmarks<T: Config<I>, I: 'static, PC: Parachain<Hash = ParaHash>>(
	header: HeaderOf<PC>,
) {
	let parachain = ParaId(PC::PARACHAIN_ID);
	let parachain_head = ParaHead(header.encode());
	let updated_head_data = T::ParaStoredHeaderDataBuilder::try_build(parachain, &parachain_head)
		.expect("failed to build stored parachain head in benchmarks");
	Pallet::<T, I>::update_parachain_head(
		parachain,
		None,
		0,
		updated_head_data,
		parachain_head.hash(),
	)
	.expect("failed to insert parachain head in benchmarks");
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use crate::mock::{
		run_test, test_relay_header, BigParachainHeader, RegularParachainHasher,
		RegularParachainHeader, RelayBlockHeader, RuntimeEvent as TestEvent, RuntimeOrigin,
		TestRuntime, UNTRACKED_PARACHAIN_ID,
	};
	use bp_test_utils::prepare_parachain_heads_proof;
	use codec::Encode;

	use bp_header_chain::{justification::GrandpaJustification, StoredHeaderGrandpaInfo};
	use bp_parachains::{
		BestParaHeadHash, BridgeParachainCall, ImportedParaHeadsKeyProvider, ParasInfoKeyProvider,
	};
	use bp_runtime::{
		BasicOperatingMode, OwnedBridgeModuleError, StorageDoubleMapKeyProvider,
		StorageMapKeyProvider,
	};
	use bp_test_utils::{
		authority_list, generate_owned_bridge_module_tests, make_default_justification,
		TEST_GRANDPA_SET_ID,
	};
	use frame_support::{
		assert_noop, assert_ok,
		dispatch::DispatchResultWithPostInfo,
		storage::generator::{StorageDoubleMap, StorageMap},
		traits::{Get, OnInitialize},
		weights::Weight,
	};
	use frame_system::{EventRecord, Pallet as System, Phase};
	use sp_core::Hasher;
	use sp_runtime::{traits::Header as HeaderT, DispatchError};

	type BridgesGrandpaPalletInstance = pallet_bridge_grandpa::Instance1;
	type WeightInfo = <TestRuntime as Config>::WeightInfo;
	type DbWeight = <TestRuntime as frame_system::Config>::DbWeight;

	pub(crate) fn initialize(state_root: RelayBlockHash) -> RelayBlockHash {
		pallet_bridge_grandpa::Pallet::<TestRuntime, BridgesGrandpaPalletInstance>::initialize(
			RuntimeOrigin::root(),
			bp_header_chain::InitializationData {
				header: Box::new(test_relay_header(0, state_root)),
				authority_list: authority_list(),
				set_id: 1,
				operating_mode: BasicOperatingMode::Normal,
			},
		)
		.unwrap();

		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();

		test_relay_header(0, state_root).hash()
	}

	fn proceed(
		num: RelayBlockNumber,
		state_root: RelayBlockHash,
	) -> (ParaHash, GrandpaJustification<RelayBlockHeader>) {
		pallet_bridge_grandpa::Pallet::<TestRuntime, BridgesGrandpaPalletInstance>::on_initialize(
			0,
		);

		let header = test_relay_header(num, state_root);
		let hash = header.hash();
		let justification = make_default_justification(&header);
		assert_ok!(
			pallet_bridge_grandpa::Pallet::<TestRuntime, BridgesGrandpaPalletInstance>::submit_finality_proof_ex(
				RuntimeOrigin::signed(1),
				Box::new(header),
				justification.clone(),
				TEST_GRANDPA_SET_ID,
			)
		);

		(hash, justification)
	}

	fn initial_best_head(parachain: u32) -> ParaInfo {
		ParaInfo {
			best_head_hash: BestParaHeadHash {
				at_relay_block_number: 0,
				head_hash: head_data(parachain, 0).hash(),
			},
			next_imported_hash_position: 1,
		}
	}

	pub(crate) fn head_data(parachain: u32, head_number: u32) -> ParaHead {
		ParaHead(
			RegularParachainHeader::new(
				head_number as _,
				Default::default(),
				RegularParachainHasher::hash(&(parachain, head_number).encode()),
				Default::default(),
				Default::default(),
			)
			.encode(),
		)
	}

	fn stored_head_data(parachain: u32, head_number: u32) -> ParaStoredHeaderData {
		ParaStoredHeaderData(
			(head_number as u64, RegularParachainHasher::hash(&(parachain, head_number).encode()))
				.encode(),
		)
	}

	fn big_head_data(parachain: u32, head_number: u32) -> ParaHead {
		ParaHead(
			BigParachainHeader::new(
				head_number as _,
				Default::default(),
				RegularParachainHasher::hash(&(parachain, head_number).encode()),
				Default::default(),
				Default::default(),
			)
			.encode(),
		)
	}

	fn big_stored_head_data(parachain: u32, head_number: u32) -> ParaStoredHeaderData {
		ParaStoredHeaderData(
			(head_number as u128, RegularParachainHasher::hash(&(parachain, head_number).encode()))
				.encode(),
		)
	}

	fn head_hash(parachain: u32, head_number: u32) -> ParaHash {
		head_data(parachain, head_number).hash()
	}

	fn import_parachain_1_head(
		relay_chain_block: RelayBlockNumber,
		relay_state_root: RelayBlockHash,
		parachains: Vec<(ParaId, ParaHash)>,
		proof: ParaHeadsProof,
	) -> DispatchResultWithPostInfo {
		Pallet::<TestRuntime>::submit_parachain_heads(
			RuntimeOrigin::signed(1),
			(relay_chain_block, test_relay_header(relay_chain_block, relay_state_root).hash()),
			parachains,
			proof,
		)
	}

	fn weight_of_import_parachain_1_head(proof: &ParaHeadsProof, prune_expected: bool) -> Weight {
		let db_weight = <TestRuntime as frame_system::Config>::DbWeight::get();
		WeightInfoOf::<TestRuntime, ()>::submit_parachain_heads_weight(db_weight, proof, 1)
			.saturating_sub(if prune_expected {
				Weight::zero()
			} else {
				WeightInfoOf::<TestRuntime, ()>::parachain_head_pruning_weight(db_weight)
			})
	}

	#[test]
	fn submit_parachain_heads_checks_operating_mode() {
		let (state_root, proof, parachains) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 0))]);

		run_test(|| {
			initialize(state_root);

			// `submit_parachain_heads()` should fail when the pallet is halted.
			PalletOperatingMode::<TestRuntime>::put(BasicOperatingMode::Halted);
			assert_noop!(
				Pallet::<TestRuntime>::submit_parachain_heads(
					RuntimeOrigin::signed(1),
					(0, test_relay_header(0, state_root).hash()),
					parachains.clone(),
					proof.clone(),
				),
				Error::<TestRuntime>::BridgeModule(OwnedBridgeModuleError::Halted)
			);

			// `submit_parachain_heads()` should succeed now that the pallet is resumed.
			PalletOperatingMode::<TestRuntime>::put(BasicOperatingMode::Normal);
			assert_ok!(Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(0, test_relay_header(0, state_root).hash()),
				parachains,
				proof,
			),);
		});
	}

	#[test]
	fn imports_initial_parachain_heads() {
		let (state_root, proof, parachains) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![
				(1, head_data(1, 0)),
				(3, head_data(3, 10)),
			]);
		run_test(|| {
			initialize(state_root);

			// we're trying to update heads of parachains 1, 2 and 3
			let expected_weight =
				WeightInfo::submit_parachain_heads_weight(DbWeight::get(), &proof, 2);
			let result = Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(0, test_relay_header(0, state_root).hash()),
				parachains,
				proof,
			);
			assert_ok!(result);
			assert_eq!(result.expect("checked above").actual_weight, Some(expected_weight));

			// but only 1 and 2 are updated, because proof is missing head of parachain#2
			assert_eq!(ParasInfo::<TestRuntime>::get(ParaId(1)), Some(initial_best_head(1)));
			assert_eq!(ParasInfo::<TestRuntime>::get(ParaId(2)), None);
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(3)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 0,
						head_hash: head_data(3, 10).hash()
					},
					next_imported_hash_position: 1,
				})
			);

			assert_eq!(
				ImportedParaHeads::<TestRuntime>::get(
					ParaId(1),
					initial_best_head(1).best_head_hash.head_hash
				)
				.map(|h| h.into_inner()),
				Some(stored_head_data(1, 0))
			);
			assert_eq!(
				ImportedParaHeads::<TestRuntime>::get(
					ParaId(2),
					initial_best_head(2).best_head_hash.head_hash
				)
				.map(|h| h.into_inner()),
				None
			);
			assert_eq!(
				ImportedParaHeads::<TestRuntime>::get(ParaId(3), head_hash(3, 10))
					.map(|h| h.into_inner()),
				Some(stored_head_data(3, 10))
			);

			assert_eq!(
				System::<TestRuntime>::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: initial_best_head(1).best_head_hash.head_hash,
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(3),
							parachain_head_hash: head_data(3, 10).hash(),
						}),
						topics: vec![],
					}
				],
			);
		});
	}

	#[test]
	fn imports_parachain_heads_is_able_to_progress() {
		let (state_root_5, proof_5, parachains_5) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 5))]);
		let (state_root_10, proof_10, parachains_10) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 10))]);
		run_test(|| {
			// start with relay block #0 and import head#5 of parachain#1
			initialize(state_root_5);
			assert_ok!(import_parachain_1_head(0, state_root_5, parachains_5, proof_5));
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(1)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 0,
						head_hash: head_data(1, 5).hash()
					},
					next_imported_hash_position: 1,
				})
			);
			assert_eq!(
				ImportedParaHeads::<TestRuntime>::get(ParaId(1), head_data(1, 5).hash())
					.map(|h| h.into_inner()),
				Some(stored_head_data(1, 5))
			);
			assert_eq!(
				ImportedParaHeads::<TestRuntime>::get(ParaId(1), head_data(1, 10).hash())
					.map(|h| h.into_inner()),
				None
			);
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::Parachains(Event::UpdatedParachainHead {
						parachain: ParaId(1),
						parachain_head_hash: head_data(1, 5).hash(),
					}),
					topics: vec![],
				}],
			);

			// import head#10 of parachain#1 at relay block #1
			let (relay_1_hash, justification) = proceed(1, state_root_10);
			assert_ok!(import_parachain_1_head(1, state_root_10, parachains_10, proof_10));
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(1)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 1,
						head_hash: head_data(1, 10).hash()
					},
					next_imported_hash_position: 2,
				})
			);
			assert_eq!(
				ImportedParaHeads::<TestRuntime>::get(ParaId(1), head_data(1, 5).hash())
					.map(|h| h.into_inner()),
				Some(stored_head_data(1, 5))
			);
			assert_eq!(
				ImportedParaHeads::<TestRuntime>::get(ParaId(1), head_data(1, 10).hash())
					.map(|h| h.into_inner()),
				Some(stored_head_data(1, 10))
			);
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: head_data(1, 5).hash(),
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Grandpa1(
							pallet_bridge_grandpa::Event::UpdatedBestFinalizedHeader {
								number: 1,
								hash: relay_1_hash,
								grandpa_info: StoredHeaderGrandpaInfo {
									finality_proof: justification,
									new_verification_context: None,
								},
							}
						),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: head_data(1, 10).hash(),
						}),
						topics: vec![],
					}
				],
			);
		});
	}

	#[test]
	fn ignores_untracked_parachain() {
		let (state_root, proof, parachains) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![
				(1, head_data(1, 5)),
				(UNTRACKED_PARACHAIN_ID, head_data(1, 5)),
				(2, head_data(1, 5)),
			]);
		run_test(|| {
			// start with relay block #0 and try to import head#5 of parachain#1 and untracked
			// parachain
			let expected_weight =
				WeightInfo::submit_parachain_heads_weight(DbWeight::get(), &proof, 3)
					.saturating_sub(WeightInfo::parachain_head_storage_write_weight(
						DbWeight::get(),
					));
			initialize(state_root);
			let result = Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(0, test_relay_header(0, state_root).hash()),
				parachains,
				proof,
			);
			assert_ok!(result);
			assert_eq!(result.expect("checked above").actual_weight, Some(expected_weight));
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(1)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 0,
						head_hash: head_data(1, 5).hash()
					},
					next_imported_hash_position: 1,
				})
			);
			assert_eq!(ParasInfo::<TestRuntime>::get(ParaId(UNTRACKED_PARACHAIN_ID)), None,);
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(2)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 0,
						head_hash: head_data(1, 5).hash()
					},
					next_imported_hash_position: 1,
				})
			);
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: head_data(1, 5).hash(),
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UntrackedParachainRejected {
							parachain: ParaId(UNTRACKED_PARACHAIN_ID),
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(2),
							parachain_head_hash: head_data(1, 5).hash(),
						}),
						topics: vec![],
					}
				],
			);
		});
	}

	#[test]
	fn does_nothing_when_already_imported_this_head_at_previous_relay_header() {
		let (state_root, proof, parachains) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 0))]);
		run_test(|| {
			// import head#0 of parachain#1 at relay block#0
			initialize(state_root);
			assert_ok!(import_parachain_1_head(0, state_root, parachains.clone(), proof.clone()));
			assert_eq!(ParasInfo::<TestRuntime>::get(ParaId(1)), Some(initial_best_head(1)));
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::Parachains(Event::UpdatedParachainHead {
						parachain: ParaId(1),
						parachain_head_hash: initial_best_head(1).best_head_hash.head_hash,
					}),
					topics: vec![],
				}],
			);

			// try to import head#0 of parachain#1 at relay block#1
			// => call succeeds, but nothing is changed
			let (relay_1_hash, justification) = proceed(1, state_root);
			assert_ok!(import_parachain_1_head(1, state_root, parachains, proof));
			assert_eq!(ParasInfo::<TestRuntime>::get(ParaId(1)), Some(initial_best_head(1)));
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: initial_best_head(1).best_head_hash.head_hash,
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Grandpa1(
							pallet_bridge_grandpa::Event::UpdatedBestFinalizedHeader {
								number: 1,
								hash: relay_1_hash,
								grandpa_info: StoredHeaderGrandpaInfo {
									finality_proof: justification,
									new_verification_context: None,
								}
							}
						),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::RejectedObsoleteParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: initial_best_head(1).best_head_hash.head_hash,
						}),
						topics: vec![],
					}
				],
			);
		});
	}

	#[test]
	fn does_nothing_when_already_imported_head_at_better_relay_header() {
		let (state_root_5, proof_5, parachains_5) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 5))]);
		let (state_root_10, proof_10, parachains_10) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 10))]);
		run_test(|| {
			// start with relay block #0
			initialize(state_root_5);

			// head#10 of parachain#1 at relay block#1
			let (relay_1_hash, justification) = proceed(1, state_root_10);
			assert_ok!(import_parachain_1_head(1, state_root_10, parachains_10, proof_10));
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(1)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 1,
						head_hash: head_data(1, 10).hash()
					},
					next_imported_hash_position: 1,
				})
			);
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Grandpa1(
							pallet_bridge_grandpa::Event::UpdatedBestFinalizedHeader {
								number: 1,
								hash: relay_1_hash,
								grandpa_info: StoredHeaderGrandpaInfo {
									finality_proof: justification.clone(),
									new_verification_context: None,
								}
							}
						),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: head_data(1, 10).hash(),
						}),
						topics: vec![],
					}
				],
			);

			// now try to import head#5 at relay block#0
			// => nothing is changed, because better head has already been imported
			assert_ok!(import_parachain_1_head(0, state_root_5, parachains_5, proof_5));
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(1)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 1,
						head_hash: head_data(1, 10).hash()
					},
					next_imported_hash_position: 1,
				})
			);
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Grandpa1(
							pallet_bridge_grandpa::Event::UpdatedBestFinalizedHeader {
								number: 1,
								hash: relay_1_hash,
								grandpa_info: StoredHeaderGrandpaInfo {
									finality_proof: justification,
									new_verification_context: None,
								}
							}
						),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: head_data(1, 10).hash(),
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::RejectedObsoleteParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: head_data(1, 5).hash(),
						}),
						topics: vec![],
					}
				],
			);
		});
	}

	#[test]
	fn does_nothing_when_parachain_head_is_too_large() {
		let (state_root, proof, parachains) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![
				(1, head_data(1, 5)),
				(4, big_head_data(1, 5)),
			]);
		run_test(|| {
			// start with relay block #0 and try to import head#5 of parachain#1 and big parachain
			initialize(state_root);
			let result = Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(0, test_relay_header(0, state_root).hash()),
				parachains,
				proof,
			);
			assert_ok!(result);
			assert_eq!(
				ParasInfo::<TestRuntime>::get(ParaId(1)),
				Some(ParaInfo {
					best_head_hash: BestParaHeadHash {
						at_relay_block_number: 0,
						head_hash: head_data(1, 5).hash()
					},
					next_imported_hash_position: 1,
				})
			);
			assert_eq!(ParasInfo::<TestRuntime>::get(ParaId(4)), None);
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::UpdatedParachainHead {
							parachain: ParaId(1),
							parachain_head_hash: head_data(1, 5).hash(),
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: TestEvent::Parachains(Event::RejectedLargeParachainHead {
							parachain: ParaId(4),
							parachain_head_hash: big_head_data(1, 5).hash(),
							parachain_head_size: big_stored_head_data(1, 5).encoded_size() as u32,
						}),
						topics: vec![],
					},
				],
			);
		});
	}

	#[test]
	fn prunes_old_heads() {
		run_test(|| {
			let heads_to_keep = crate::mock::HeadsToKeep::get();

			// import exactly `HeadsToKeep` headers
			for i in 0..heads_to_keep {
				let (state_root, proof, parachains) = prepare_parachain_heads_proof::<
					RegularParachainHeader,
				>(vec![(1, head_data(1, i))]);
				if i == 0 {
					initialize(state_root);
				} else {
					proceed(i, state_root);
				}

				let expected_weight = weight_of_import_parachain_1_head(&proof, false);
				let result = import_parachain_1_head(i, state_root, parachains, proof);
				assert_ok!(result);
				assert_eq!(result.expect("checked above").actual_weight, Some(expected_weight));
			}

			// nothing is pruned yet
			for i in 0..heads_to_keep {
				assert!(ImportedParaHeads::<TestRuntime>::get(ParaId(1), head_data(1, i).hash())
					.is_some());
			}

			// import next relay chain header and next parachain head
			let (state_root, proof, parachains) = prepare_parachain_heads_proof::<
				RegularParachainHeader,
			>(vec![(1, head_data(1, heads_to_keep))]);
			proceed(heads_to_keep, state_root);
			let expected_weight = weight_of_import_parachain_1_head(&proof, true);
			let result = import_parachain_1_head(heads_to_keep, state_root, parachains, proof);
			assert_ok!(result);
			assert_eq!(result.expect("checked above").actual_weight, Some(expected_weight));

			// and the head#0 is pruned
			assert!(
				ImportedParaHeads::<TestRuntime>::get(ParaId(1), head_data(1, 0).hash()).is_none()
			);
			for i in 1..=heads_to_keep {
				assert!(ImportedParaHeads::<TestRuntime>::get(ParaId(1), head_data(1, i).hash())
					.is_some());
			}
		});
	}

	#[test]
	fn fails_on_unknown_relay_chain_block() {
		let (state_root, proof, parachains) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 5))]);
		run_test(|| {
			// start with relay block #0
			initialize(state_root);

			// try to import head#5 of parachain#1 at unknown relay chain block #1
			assert_noop!(
				import_parachain_1_head(1, state_root, parachains, proof),
				Error::<TestRuntime>::UnknownRelayChainBlock
			);
		});
	}

	#[test]
	fn fails_on_invalid_storage_proof() {
		let (_state_root, proof, parachains) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 5))]);
		run_test(|| {
			// start with relay block #0
			initialize(Default::default());

			// try to import head#5 of parachain#1 at relay chain block #0
			assert_noop!(
				import_parachain_1_head(0, Default::default(), parachains, proof),
				Error::<TestRuntime>::HeaderChainStorageProof(HeaderChainError::StorageProof(
					StorageProofError::StorageRootMismatch
				))
			);
		});
	}

	#[test]
	fn is_not_rewriting_existing_head_if_failed_to_read_updated_head() {
		let (state_root_5, proof_5, parachains_5) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 5))]);
		let (state_root_10_at_20, proof_10_at_20, parachains_10_at_20) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(2, head_data(2, 10))]);
		let (state_root_10_at_30, proof_10_at_30, parachains_10_at_30) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 10))]);
		run_test(|| {
			// we've already imported head#5 of parachain#1 at relay block#10
			initialize(state_root_5);
			import_parachain_1_head(0, state_root_5, parachains_5, proof_5).expect("ok");
			assert_eq!(
				Pallet::<TestRuntime>::best_parachain_head(ParaId(1)),
				Some(stored_head_data(1, 5))
			);

			// then if someone is pretending to provide updated head#10 of parachain#1 at relay
			// block#20, but fails to do that
			//
			// => we'll leave previous value
			proceed(20, state_root_10_at_20);
			assert_ok!(Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(20, test_relay_header(20, state_root_10_at_20).hash()),
				parachains_10_at_20,
				proof_10_at_20,
			),);
			assert_eq!(
				Pallet::<TestRuntime>::best_parachain_head(ParaId(1)),
				Some(stored_head_data(1, 5))
			);

			// then if someone is pretending to provide updated head#10 of parachain#1 at relay
			// block#30, and actually provides it
			//
			// => we'll update value
			proceed(30, state_root_10_at_30);
			assert_ok!(Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(30, test_relay_header(30, state_root_10_at_30).hash()),
				parachains_10_at_30,
				proof_10_at_30,
			),);
			assert_eq!(
				Pallet::<TestRuntime>::best_parachain_head(ParaId(1)),
				Some(stored_head_data(1, 10))
			);
		});
	}

	#[test]
	fn storage_keys_computed_properly() {
		assert_eq!(
			ParasInfo::<TestRuntime>::storage_map_final_key(ParaId(42)).to_vec(),
			ParasInfoKeyProvider::final_key("Parachains", &ParaId(42)).0
		);

		assert_eq!(
			ImportedParaHeads::<TestRuntime>::storage_double_map_final_key(
				ParaId(42),
				ParaHash::from([21u8; 32])
			)
			.to_vec(),
			ImportedParaHeadsKeyProvider::final_key(
				"Parachains",
				&ParaId(42),
				&ParaHash::from([21u8; 32])
			)
			.0,
		);
	}

	#[test]
	fn ignores_parachain_head_if_it_is_missing_from_storage_proof() {
		let (state_root, proof, _) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![]);
		let parachains = vec![(ParaId(2), Default::default())];
		run_test(|| {
			initialize(state_root);
			assert_ok!(Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(0, test_relay_header(0, state_root).hash()),
				parachains,
				proof,
			));
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::Parachains(Event::MissingParachainHead {
						parachain: ParaId(2),
					}),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn ignores_parachain_head_if_parachain_head_hash_is_wrong() {
		let (state_root, proof, _) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 0))]);
		let parachains = vec![(ParaId(1), head_data(1, 10).hash())];
		run_test(|| {
			initialize(state_root);
			assert_ok!(Pallet::<TestRuntime>::submit_parachain_heads(
				RuntimeOrigin::signed(1),
				(0, test_relay_header(0, state_root).hash()),
				parachains,
				proof,
			));
			assert_eq!(
				System::<TestRuntime>::events(),
				vec![EventRecord {
					phase: Phase::Initialization,
					event: TestEvent::Parachains(Event::IncorrectParachainHeadHash {
						parachain: ParaId(1),
						parachain_head_hash: head_data(1, 10).hash(),
						actual_parachain_head_hash: head_data(1, 0).hash(),
					}),
					topics: vec![],
				}],
			);
		});
	}

	#[test]
	fn test_bridge_parachain_call_is_correctly_defined() {
		let (state_root, proof, _) =
			prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 0))]);
		let parachains = vec![(ParaId(2), Default::default())];
		let relay_header_id = (0, test_relay_header(0, state_root).hash());

		let direct_submit_parachain_heads_call = Call::<TestRuntime>::submit_parachain_heads {
			at_relay_block: relay_header_id,
			parachains: parachains.clone(),
			parachain_heads_proof: proof.clone(),
		};
		let indirect_submit_parachain_heads_call = BridgeParachainCall::submit_parachain_heads {
			at_relay_block: relay_header_id,
			parachains,
			parachain_heads_proof: proof,
		};
		assert_eq!(
			direct_submit_parachain_heads_call.encode(),
			indirect_submit_parachain_heads_call.encode()
		);
	}

	generate_owned_bridge_module_tests!(BasicOperatingMode::Normal, BasicOperatingMode::Halted);

	#[test]
	fn maybe_max_parachains_returns_correct_value() {
		assert_eq!(MaybeMaxParachains::<TestRuntime, ()>::get(), Some(mock::TOTAL_PARACHAINS));
	}

	#[test]
	fn maybe_max_total_parachain_hashes_returns_correct_value() {
		assert_eq!(
			MaybeMaxTotalParachainHashes::<TestRuntime, ()>::get(),
			Some(mock::TOTAL_PARACHAINS * mock::HeadsToKeep::get()),
		);
	}

	#[test]
	fn submit_finality_proof_requires_signed_origin() {
		run_test(|| {
			let (state_root, proof, parachains) =
				prepare_parachain_heads_proof::<RegularParachainHeader>(vec![(1, head_data(1, 0))]);

			initialize(state_root);

			// `submit_parachain_heads()` should fail when the pallet is halted.
			assert_noop!(
				Pallet::<TestRuntime>::submit_parachain_heads(
					RuntimeOrigin::root(),
					(0, test_relay_header(0, state_root).hash()),
					parachains,
					proof,
				),
				DispatchError::BadOrigin
			);
		})
	}
}
