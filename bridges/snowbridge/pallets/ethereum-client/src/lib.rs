// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Ethereum Beacon Client
//!
//! A light client that verifies consensus updates signed by the sync committee of the beacon chain.
//!
//! # Extrinsics
//!
//! ## Governance
//!
//! * [`Call::force_checkpoint`]: Set the initial trusted consensus checkpoint.
//! * [`Call::set_operating_mode`]: Set the operating mode of the pallet. Can be used to disable
//!   processing of conensus updates.
//!
//! ## Consensus Updates
//!
//! * [`Call::submit`]: Submit a finalized beacon header with an optional sync committee update
//! * [`Call::submit_execution_header`]: Submit an execution header together with an ancestry proof
//!   that can be verified against an already imported finalized beacon header.
#![cfg_attr(not(feature = "std"), no_std)]

pub mod config;
pub mod functions;
pub mod impls;
pub mod types;
pub mod weights;

#[cfg(any(test, feature = "fuzzing"))]
pub mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::{
	dispatch::DispatchResult, pallet_prelude::OptionQuery, traits::Get, transactional,
};
use frame_system::ensure_signed;
use primitives::{
	fast_aggregate_verify, verify_merkle_branch, verify_receipt_proof, BeaconHeader, BlsError,
	CompactBeaconState, CompactExecutionHeader, ExecutionHeaderState, ForkData, ForkVersion,
	ForkVersions, PublicKeyPrepared, SigningData,
};
use snowbridge_core::{BasicOperatingMode, RingBufferMap};
use sp_core::H256;
use sp_std::prelude::*;
pub use weights::WeightInfo;

use functions::{
	compute_epoch, compute_period, decompress_sync_committee_bits, sync_committee_sum,
};
pub use types::ExecutionHeaderBuffer;
use types::{
	CheckpointUpdate, ExecutionHeaderUpdate, FinalizedBeaconStateBuffer, SyncCommitteePrepared,
	Update,
};

pub use pallet::*;

pub use config::SLOTS_PER_HISTORICAL_ROOT;

pub const LOG_TARGET: &str = "ethereum-client";

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[derive(scale_info::TypeInfo, codec::Encode, codec::Decode, codec::MaxEncodedLen)]
	#[codec(mel_bound(T: Config))]
	#[scale_info(skip_type_params(T))]
	pub struct MaxFinalizedHeadersToKeep<T: Config>(PhantomData<T>);
	impl<T: Config> Get<u32> for MaxFinalizedHeadersToKeep<T> {
		fn get() -> u32 {
			// Consider max latency allowed between LatestFinalizedState and LatestExecutionState is
			// the total slots in one sync_committee_period so 1 should be fine we keep 2 periods
			// here for redundancy.
			const MAX_REDUNDANCY: u32 = 2;
			config::EPOCHS_PER_SYNC_COMMITTEE_PERIOD as u32 * MAX_REDUNDANCY
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		#[pallet::constant]
		type ForkVersions: Get<ForkVersions>;
		/// Maximum number of execution headers to keep
		#[pallet::constant]
		type MaxExecutionHeadersToKeep: Get<u32>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		BeaconHeaderImported {
			block_hash: H256,
			slot: u64,
		},
		ExecutionHeaderImported {
			block_hash: H256,
			block_number: u64,
		},
		SyncCommitteeUpdated {
			period: u64,
		},
		/// Set OperatingMode
		OperatingModeChanged {
			mode: BasicOperatingMode,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		SkippedSyncCommitteePeriod,
		/// Attested header is older than latest finalized header.
		IrrelevantUpdate,
		NotBootstrapped,
		SyncCommitteeParticipantsNotSupermajority,
		InvalidHeaderMerkleProof,
		InvalidSyncCommitteeMerkleProof,
		InvalidExecutionHeaderProof,
		InvalidAncestryMerkleProof,
		InvalidBlockRootsRootMerkleProof,
		HeaderNotFinalized,
		BlockBodyHashTreeRootFailed,
		HeaderHashTreeRootFailed,
		SyncCommitteeHashTreeRootFailed,
		SigningRootHashTreeRootFailed,
		ForkDataHashTreeRootFailed,
		ExpectedFinalizedHeaderNotStored,
		BLSPreparePublicKeysFailed,
		BLSVerificationFailed(BlsError),
		InvalidUpdateSlot,
		/// The given update is not in the expected period, or the given next sync committee does
		/// not match the next sync committee in storage.
		InvalidSyncCommitteeUpdate,
		ExecutionHeaderTooFarBehind,
		ExecutionHeaderSkippedBlock,
		Halted,
	}

	/// Latest imported checkpoint root
	#[pallet::storage]
	#[pallet::getter(fn initial_checkpoint_root)]
	pub(super) type InitialCheckpointRoot<T: Config> = StorageValue<_, H256, ValueQuery>;

	/// Latest imported finalized block root
	#[pallet::storage]
	#[pallet::getter(fn latest_finalized_block_root)]
	pub(super) type LatestFinalizedBlockRoot<T: Config> = StorageValue<_, H256, ValueQuery>;

	/// Beacon state by finalized block root
	#[pallet::storage]
	#[pallet::getter(fn finalized_beacon_state)]
	pub(super) type FinalizedBeaconState<T: Config> =
		StorageMap<_, Identity, H256, CompactBeaconState, OptionQuery>;

	/// Finalized Headers: Current position in ring buffer
	#[pallet::storage]
	pub(crate) type FinalizedBeaconStateIndex<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Finalized Headers: Mapping of ring buffer index to a pruning candidate
	#[pallet::storage]
	pub(crate) type FinalizedBeaconStateMapping<T: Config> =
		StorageMap<_, Identity, u32, H256, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn validators_root)]
	pub(super) type ValidatorsRoot<T: Config> = StorageValue<_, H256, ValueQuery>;

	/// Sync committee for current period
	#[pallet::storage]
	pub(super) type CurrentSyncCommittee<T: Config> =
		StorageValue<_, SyncCommitteePrepared, ValueQuery>;

	/// Sync committee for next period
	#[pallet::storage]
	pub(super) type NextSyncCommittee<T: Config> =
		StorageValue<_, SyncCommitteePrepared, ValueQuery>;

	/// Latest imported execution header
	#[pallet::storage]
	#[pallet::getter(fn latest_execution_state)]
	pub(super) type LatestExecutionState<T: Config> =
		StorageValue<_, ExecutionHeaderState, ValueQuery>;

	/// Execution Headers
	#[pallet::storage]
	pub type ExecutionHeaders<T: Config> =
		StorageMap<_, Identity, H256, CompactExecutionHeader, OptionQuery>;

	/// Execution Headers: Current position in ring buffer
	#[pallet::storage]
	pub type ExecutionHeaderIndex<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Execution Headers: Mapping of ring buffer index to a pruning candidate
	#[pallet::storage]
	pub type ExecutionHeaderMapping<T: Config> = StorageMap<_, Identity, u32, H256, ValueQuery>;

	/// The current operating mode of the pallet.
	#[pallet::storage]
	#[pallet::getter(fn operating_mode)]
	pub type OperatingMode<T: Config> = StorageValue<_, BasicOperatingMode, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::force_checkpoint())]
		#[transactional]
		/// Used for pallet initialization and light client resetting. Needs to be called by
		/// the root origin.
		pub fn force_checkpoint(
			origin: OriginFor<T>,
			update: Box<CheckpointUpdate>,
		) -> DispatchResult {
			ensure_root(origin)?;
			Self::process_checkpoint_update(&update)?;
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight({
			match update.next_sync_committee_update {
				None => T::WeightInfo::submit(),
				Some(_) => T::WeightInfo::submit_with_sync_committee(),
			}
		})]
		#[transactional]
		/// Submits a new finalized beacon header update. The update may contain the next
		/// sync committee.
		pub fn submit(origin: OriginFor<T>, update: Box<Update>) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!Self::operating_mode().is_halted(), Error::<T>::Halted);
			Self::process_update(&update)?;
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::submit_execution_header())]
		#[transactional]
		/// Submits a new execution header update. The relevant related beacon header
		/// is also included to prove the execution header, as well as ancestry proof data.
		pub fn submit_execution_header(
			origin: OriginFor<T>,
			update: Box<ExecutionHeaderUpdate>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!Self::operating_mode().is_halted(), Error::<T>::Halted);
			Self::process_execution_header_update(&update)?;
			Ok(())
		}

		/// Halt or resume all pallet operations. May only be called by root.
		#[pallet::call_index(3)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operating_mode(
			origin: OriginFor<T>,
			mode: BasicOperatingMode,
		) -> DispatchResult {
			ensure_root(origin)?;
			OperatingMode::<T>::set(mode);
			Self::deposit_event(Event::OperatingModeChanged { mode });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Forces a finalized beacon header checkpoint update. The current sync committee,
		/// with a header attesting to the current sync committee, should be provided.
		/// An `block_roots` proof should also be provided. This is used for ancestry proofs
		/// for execution header updates.
		pub(crate) fn process_checkpoint_update(update: &CheckpointUpdate) -> DispatchResult {
			let sync_committee_root = update
				.current_sync_committee
				.hash_tree_root()
				.map_err(|_| Error::<T>::SyncCommitteeHashTreeRootFailed)?;

			// Verifies the sync committee in the Beacon state.
			ensure!(
				verify_merkle_branch(
					sync_committee_root,
					&update.current_sync_committee_branch,
					config::CURRENT_SYNC_COMMITTEE_SUBTREE_INDEX,
					config::CURRENT_SYNC_COMMITTEE_DEPTH,
					update.header.state_root
				),
				Error::<T>::InvalidSyncCommitteeMerkleProof
			);

			let header_root: H256 = update
				.header
				.hash_tree_root()
				.map_err(|_| Error::<T>::HeaderHashTreeRootFailed)?;

			// This is used for ancestry proofs in ExecutionHeader updates. This verifies the
			// BeaconState: the beacon state root is the tree root; the `block_roots` hash is the
			// tree leaf.
			ensure!(
				verify_merkle_branch(
					update.block_roots_root,
					&update.block_roots_branch,
					config::BLOCK_ROOTS_SUBTREE_INDEX,
					config::BLOCK_ROOTS_DEPTH,
					update.header.state_root
				),
				Error::<T>::InvalidBlockRootsRootMerkleProof
			);

			let sync_committee_prepared: SyncCommitteePrepared = (&update.current_sync_committee)
				.try_into()
				.map_err(|_| <Error<T>>::BLSPreparePublicKeysFailed)?;
			<CurrentSyncCommittee<T>>::set(sync_committee_prepared);
			<NextSyncCommittee<T>>::kill();
			InitialCheckpointRoot::<T>::set(header_root);
			<LatestExecutionState<T>>::kill();

			Self::store_validators_root(update.validators_root);
			Self::store_finalized_header(header_root, update.header, update.block_roots_root)?;

			Ok(())
		}

		pub(crate) fn process_update(update: &Update) -> DispatchResult {
			Self::cross_check_execution_state()?;
			Self::verify_update(update)?;
			Self::apply_update(update)?;
			Ok(())
		}

		/// Cross check to make sure that execution header import does not fall too far behind
		/// finalised beacon header import. If that happens just return an error and pause
		/// processing until execution header processing has caught up.
		pub(crate) fn cross_check_execution_state() -> DispatchResult {
			let latest_finalized_state =
				FinalizedBeaconState::<T>::get(LatestFinalizedBlockRoot::<T>::get())
					.ok_or(Error::<T>::NotBootstrapped)?;
			let latest_execution_state = Self::latest_execution_state();
			// The execution header import should be at least within the slot range of a sync
			// committee period.
			let max_latency = config::EPOCHS_PER_SYNC_COMMITTEE_PERIOD * config::SLOTS_PER_EPOCH;
			ensure!(
				latest_execution_state.beacon_slot == 0 ||
					latest_finalized_state.slot <
						latest_execution_state.beacon_slot + max_latency as u64,
				Error::<T>::ExecutionHeaderTooFarBehind
			);
			Ok(())
		}

		/// References and strictly follows <https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#validate_light_client_update>
		/// Verifies that provided next sync committee is valid through a series of checks
		/// (including checking that a sync committee period isn't skipped and that the header is
		/// signed by the current sync committee.
		fn verify_update(update: &Update) -> DispatchResult {
			// Verify sync committee has sufficient participants.
			let participation =
				decompress_sync_committee_bits(update.sync_aggregate.sync_committee_bits);
			Self::sync_committee_participation_is_supermajority(&participation)?;

			// Verify update does not skip a sync committee period.
			ensure!(
				update.signature_slot > update.attested_header.slot &&
					update.attested_header.slot >= update.finalized_header.slot,
				Error::<T>::InvalidUpdateSlot
			);
			// Retrieve latest finalized state.
			let latest_finalized_state =
				FinalizedBeaconState::<T>::get(LatestFinalizedBlockRoot::<T>::get())
					.ok_or(Error::<T>::NotBootstrapped)?;
			let store_period = compute_period(latest_finalized_state.slot);
			let signature_period = compute_period(update.signature_slot);
			if <NextSyncCommittee<T>>::exists() {
				ensure!(
					(store_period..=store_period + 1).contains(&signature_period),
					Error::<T>::SkippedSyncCommitteePeriod
				)
			} else {
				ensure!(signature_period == store_period, Error::<T>::SkippedSyncCommitteePeriod)
			}

			// Verify update is relevant.
			let update_attested_period = compute_period(update.attested_header.slot);
			let update_has_next_sync_committee = !<NextSyncCommittee<T>>::exists() &&
				(update.next_sync_committee_update.is_some() &&
					update_attested_period == store_period);
			ensure!(
				update.attested_header.slot > latest_finalized_state.slot ||
					update_has_next_sync_committee,
				Error::<T>::IrrelevantUpdate
			);

			// Verify that the `finality_branch`, if present, confirms `finalized_header` to match
			// the finalized checkpoint root saved in the state of `attested_header`.
			let finalized_block_root: H256 = update
				.finalized_header
				.hash_tree_root()
				.map_err(|_| Error::<T>::HeaderHashTreeRootFailed)?;
			ensure!(
				verify_merkle_branch(
					finalized_block_root,
					&update.finality_branch,
					config::FINALIZED_ROOT_SUBTREE_INDEX,
					config::FINALIZED_ROOT_DEPTH,
					update.attested_header.state_root
				),
				Error::<T>::InvalidHeaderMerkleProof
			);

			// Though following check does not belong to ALC spec we verify block_roots_root to
			// match the finalized checkpoint root saved in the state of `finalized_header` so to
			// cache it for later use in `verify_ancestry_proof`.
			ensure!(
				verify_merkle_branch(
					update.block_roots_root,
					&update.block_roots_branch,
					config::BLOCK_ROOTS_SUBTREE_INDEX,
					config::BLOCK_ROOTS_DEPTH,
					update.finalized_header.state_root
				),
				Error::<T>::InvalidBlockRootsRootMerkleProof
			);

			// Verify that the `next_sync_committee`, if present, actually is the next sync
			// committee saved in the state of the `attested_header`.
			if let Some(next_sync_committee_update) = &update.next_sync_committee_update {
				let sync_committee_root = next_sync_committee_update
					.next_sync_committee
					.hash_tree_root()
					.map_err(|_| Error::<T>::SyncCommitteeHashTreeRootFailed)?;
				if update_attested_period == store_period && <NextSyncCommittee<T>>::exists() {
					let next_committee_root = <NextSyncCommittee<T>>::get().root;
					ensure!(
						sync_committee_root == next_committee_root,
						Error::<T>::InvalidSyncCommitteeUpdate
					);
				}
				ensure!(
					verify_merkle_branch(
						sync_committee_root,
						&next_sync_committee_update.next_sync_committee_branch,
						config::NEXT_SYNC_COMMITTEE_SUBTREE_INDEX,
						config::NEXT_SYNC_COMMITTEE_DEPTH,
						update.attested_header.state_root
					),
					Error::<T>::InvalidSyncCommitteeMerkleProof
				);
			}

			// Verify sync committee aggregate signature.
			let sync_committee = if signature_period == store_period {
				<CurrentSyncCommittee<T>>::get()
			} else {
				<NextSyncCommittee<T>>::get()
			};
			let absent_pubkeys =
				Self::find_pubkeys(&participation, (*sync_committee.pubkeys).as_ref(), false);
			let signing_root = Self::signing_root(
				&update.attested_header,
				Self::validators_root(),
				update.signature_slot,
			)?;
			// Improvement here per <https://eth2book.info/capella/part2/building_blocks/signatures/#sync-aggregates>
			// suggested start from the full set aggregate_pubkey then subtracting the absolute
			// minority that did not participate.
			fast_aggregate_verify(
				&sync_committee.aggregate_pubkey,
				&absent_pubkeys,
				signing_root,
				&update.sync_aggregate.sync_committee_signature,
			)
			.map_err(|e| Error::<T>::BLSVerificationFailed(e))?;

			Ok(())
		}

		/// Reference and strictly follows <https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#apply_light_client_update
		/// Applies a finalized beacon header update to the beacon client. If a next sync committee
		/// is present in the update, verify the sync committee by converting it to a
		/// SyncCommitteePrepared type. Stores the provided finalized header.
		fn apply_update(update: &Update) -> DispatchResult {
			let latest_finalized_state =
				FinalizedBeaconState::<T>::get(LatestFinalizedBlockRoot::<T>::get())
					.ok_or(Error::<T>::NotBootstrapped)?;
			if let Some(next_sync_committee_update) = &update.next_sync_committee_update {
				let store_period = compute_period(latest_finalized_state.slot);
				let update_finalized_period = compute_period(update.finalized_header.slot);
				let sync_committee_prepared: SyncCommitteePrepared = (&next_sync_committee_update
					.next_sync_committee)
					.try_into()
					.map_err(|_| <Error<T>>::BLSPreparePublicKeysFailed)?;

				if !<NextSyncCommittee<T>>::exists() {
					ensure!(
						update_finalized_period == store_period,
						<Error<T>>::InvalidSyncCommitteeUpdate
					);
					<NextSyncCommittee<T>>::set(sync_committee_prepared);
				} else if update_finalized_period == store_period + 1 {
					<CurrentSyncCommittee<T>>::set(<NextSyncCommittee<T>>::get());
					<NextSyncCommittee<T>>::set(sync_committee_prepared);
				}
				log::info!(
					target: LOG_TARGET,
					"💫 SyncCommitteeUpdated at period {}.",
					update_finalized_period
				);
				Self::deposit_event(Event::SyncCommitteeUpdated {
					period: update_finalized_period,
				});
			};

			if update.finalized_header.slot > latest_finalized_state.slot {
				let finalized_block_root: H256 = update
					.finalized_header
					.hash_tree_root()
					.map_err(|_| Error::<T>::HeaderHashTreeRootFailed)?;
				Self::store_finalized_header(
					finalized_block_root,
					update.finalized_header,
					update.block_roots_root,
				)?;
			}

			Ok(())
		}

		/// Validates an execution header for import. The beacon header containing the execution
		/// header is sent, plus the execution header, along with a proof that the execution header
		/// is rooted in the beacon header body.
		pub(crate) fn process_execution_header_update(
			update: &ExecutionHeaderUpdate,
		) -> DispatchResult {
			let latest_finalized_state =
				FinalizedBeaconState::<T>::get(LatestFinalizedBlockRoot::<T>::get())
					.ok_or(Error::<T>::NotBootstrapped)?;
			// Checks that the header is an ancestor of a finalized header, using slot number.
			ensure!(
				update.header.slot <= latest_finalized_state.slot,
				Error::<T>::HeaderNotFinalized
			);

			// Checks that we don't skip execution headers, they need to be imported sequentially.
			let latest_execution_state: ExecutionHeaderState = Self::latest_execution_state();
			ensure!(
				latest_execution_state.block_number == 0 ||
					update.execution_header.block_number() ==
						latest_execution_state.block_number + 1,
				Error::<T>::ExecutionHeaderSkippedBlock
			);

			// Gets the hash tree root of the execution header, in preparation for the execution
			// header proof (used to check that the execution header is rooted in the beacon
			// header body.
			let execution_header_root: H256 = update
				.execution_header
				.hash_tree_root()
				.map_err(|_| Error::<T>::BlockBodyHashTreeRootFailed)?;

			ensure!(
				verify_merkle_branch(
					execution_header_root,
					&update.execution_branch,
					config::EXECUTION_HEADER_SUBTREE_INDEX,
					config::EXECUTION_HEADER_DEPTH,
					update.header.body_root
				),
				Error::<T>::InvalidExecutionHeaderProof
			);

			let block_root: H256 = update
				.header
				.hash_tree_root()
				.map_err(|_| Error::<T>::HeaderHashTreeRootFailed)?;

			match &update.ancestry_proof {
				Some(proof) => {
					Self::verify_ancestry_proof(
						block_root,
						update.header.slot,
						&proof.header_branch,
						proof.finalized_block_root,
					)?;
				},
				None => {
					// If the ancestry proof is not provided, we expect this header to be a
					// finalized header. We need to check that the header hash matches the finalized
					// header root at the expected slot.
					let state = <FinalizedBeaconState<T>>::get(block_root)
						.ok_or(Error::<T>::ExpectedFinalizedHeaderNotStored)?;
					if update.header.slot != state.slot {
						return Err(Error::<T>::ExpectedFinalizedHeaderNotStored.into())
					}
				},
			}

			Self::store_execution_header(
				update.execution_header.block_hash(),
				update.execution_header.clone().into(),
				update.header.slot,
				block_root,
			);

			Ok(())
		}

		/// Verify that `block_root` is an ancestor of `finalized_block_root` Used to prove that
		/// an execution header is an ancestor of a finalized header (i.e. the blocks are
		/// on the same chain).
		fn verify_ancestry_proof(
			block_root: H256,
			block_slot: u64,
			block_root_proof: &[H256],
			finalized_block_root: H256,
		) -> DispatchResult {
			let state = <FinalizedBeaconState<T>>::get(finalized_block_root)
				.ok_or(Error::<T>::ExpectedFinalizedHeaderNotStored)?;

			ensure!(block_slot < state.slot, Error::<T>::HeaderNotFinalized);

			let index_in_array = block_slot % (SLOTS_PER_HISTORICAL_ROOT as u64);
			let leaf_index = (SLOTS_PER_HISTORICAL_ROOT as u64) + index_in_array;

			ensure!(
				verify_merkle_branch(
					block_root,
					block_root_proof,
					leaf_index as usize,
					config::BLOCK_ROOT_AT_INDEX_DEPTH,
					state.block_roots_root
				),
				Error::<T>::InvalidAncestryMerkleProof
			);

			Ok(())
		}

		/// Computes the signing root for a given beacon header and domain. The hash tree root
		/// of the beacon header is computed, and then the combination of the beacon header hash
		/// and the domain makes up the signing root.
		pub(super) fn compute_signing_root(
			beacon_header: &BeaconHeader,
			domain: H256,
		) -> Result<H256, DispatchError> {
			let beacon_header_root = beacon_header
				.hash_tree_root()
				.map_err(|_| Error::<T>::HeaderHashTreeRootFailed)?;

			let hash_root = SigningData { object_root: beacon_header_root, domain }
				.hash_tree_root()
				.map_err(|_| Error::<T>::SigningRootHashTreeRootFailed)?;

			Ok(hash_root)
		}

		/// Stores a compacted (slot and block roots root (hash of the `block_roots` beacon state
		/// field, used for ancestry proof)) beacon state in a ring buffer map, with the header root
		/// as map key.
		fn store_finalized_header(
			header_root: H256,
			header: BeaconHeader,
			block_roots_root: H256,
		) -> DispatchResult {
			let slot = header.slot;

			<FinalizedBeaconStateBuffer<T>>::insert(
				header_root,
				CompactBeaconState { slot: header.slot, block_roots_root },
			);
			<LatestFinalizedBlockRoot<T>>::set(header_root);

			log::info!(
				target: LOG_TARGET,
				"💫 Updated latest finalized block root {} at slot {}.",
				header_root,
				slot
			);

			Self::deposit_event(Event::BeaconHeaderImported { block_hash: header_root, slot });

			Ok(())
		}

		/// Stores the provided execution header in pallet storage. The header is stored
		/// in a ring buffer map, with the block hash as map key. The last imported execution
		/// header is also kept in storage, for the relayer to check import progress.
		pub fn store_execution_header(
			block_hash: H256,
			header: CompactExecutionHeader,
			beacon_slot: u64,
			beacon_block_root: H256,
		) {
			let block_number = header.block_number;

			<ExecutionHeaderBuffer<T>>::insert(block_hash, header);

			log::trace!(
				target: LOG_TARGET,
				"💫 Updated latest execution block at {} to number {}.",
				block_hash,
				block_number
			);

			LatestExecutionState::<T>::mutate(|s| {
				s.beacon_block_root = beacon_block_root;
				s.beacon_slot = beacon_slot;
				s.block_hash = block_hash;
				s.block_number = block_number;
			});

			Self::deposit_event(Event::ExecutionHeaderImported { block_hash, block_number });
		}

		/// Stores the validators root in storage. Validators root is the hash tree root of all the
		/// validators at genesis and is used to used to identify the chain that we are on
		/// (used in conjunction with the fork version).
		/// <https://eth2book.info/capella/part3/containers/state/#genesis_validators_root>
		fn store_validators_root(validators_root: H256) {
			<ValidatorsRoot<T>>::set(validators_root);
		}

		/// Returns the domain for the domain_type and fork_version. The domain is used to
		/// distinguish between the different players in the chain (see DomainTypes
		/// <https://eth2book.info/capella/part3/config/constants/#domain-types>) and to ensure we are
		/// addressing the correct chain.
		/// <https://eth2book.info/capella/part3/helper/misc/#compute_domain>
		pub(super) fn compute_domain(
			domain_type: Vec<u8>,
			fork_version: ForkVersion,
			genesis_validators_root: H256,
		) -> Result<H256, DispatchError> {
			let fork_data_root =
				Self::compute_fork_data_root(fork_version, genesis_validators_root)?;

			let mut domain = [0u8; 32];
			domain[0..4].copy_from_slice(&(domain_type));
			domain[4..32].copy_from_slice(&(fork_data_root.0[..28]));

			Ok(domain.into())
		}

		/// Computes the fork data root. The fork data root is a merkleization of the current
		/// fork version and the genesis validators root.
		fn compute_fork_data_root(
			current_version: ForkVersion,
			genesis_validators_root: H256,
		) -> Result<H256, DispatchError> {
			let hash_root = ForkData {
				current_version,
				genesis_validators_root: genesis_validators_root.into(),
			}
			.hash_tree_root()
			.map_err(|_| Error::<T>::ForkDataHashTreeRootFailed)?;

			Ok(hash_root)
		}

		/// Checks that the sync committee bits (the votes of the sync committee members,
		/// represented by bits 0 and 1) is more than a supermajority (2/3 of the votes are
		/// positive).
		pub(super) fn sync_committee_participation_is_supermajority(
			sync_committee_bits: &[u8],
		) -> DispatchResult {
			let sync_committee_sum = sync_committee_sum(sync_committee_bits);
			ensure!(
				((sync_committee_sum * 3) as usize) >= sync_committee_bits.len() * 2,
				Error::<T>::SyncCommitteeParticipantsNotSupermajority
			);

			Ok(())
		}

		/// Returns the fork version based on the current epoch. The hard fork versions
		/// are defined in pallet config.
		pub(super) fn compute_fork_version(epoch: u64) -> ForkVersion {
			Self::select_fork_version(&T::ForkVersions::get(), epoch)
		}

		/// Returns the fork version based on the current epoch.
		pub(super) fn select_fork_version(fork_versions: &ForkVersions, epoch: u64) -> ForkVersion {
			if epoch >= fork_versions.deneb.epoch {
				return fork_versions.deneb.version
			}
			if epoch >= fork_versions.capella.epoch {
				return fork_versions.capella.version
			}
			if epoch >= fork_versions.bellatrix.epoch {
				return fork_versions.bellatrix.version
			}
			if epoch >= fork_versions.altair.epoch {
				return fork_versions.altair.version
			}
			fork_versions.genesis.version
		}

		/// Returns a vector of public keys that participated in the sync committee block signage.
		/// Sync committee bits is an array of 0s and 1s, 0 meaning the corresponding sync committee
		/// member did not participate in the vote, 1 meaning they participated.
		/// This method can find the absent or participating members, based on the participant
		/// parameter. participant = false will return absent participants, participant = true will
		/// return participating members.
		pub fn find_pubkeys(
			sync_committee_bits: &[u8],
			sync_committee_pubkeys: &[PublicKeyPrepared],
			participant: bool,
		) -> Vec<PublicKeyPrepared> {
			let mut pubkeys: Vec<PublicKeyPrepared> = Vec::new();
			for (bit, pubkey) in sync_committee_bits.iter().zip(sync_committee_pubkeys.iter()) {
				if *bit == u8::from(participant) {
					pubkeys.push(pubkey.clone());
				}
			}
			pubkeys
		}

		/// Calculates signing root for BeaconHeader. The signing root is used for the message
		/// value in BLS signature verification.
		pub fn signing_root(
			header: &BeaconHeader,
			validators_root: H256,
			signature_slot: u64,
		) -> Result<H256, DispatchError> {
			let fork_version = Self::compute_fork_version(compute_epoch(
				signature_slot,
				config::SLOTS_PER_EPOCH as u64,
			));
			let domain_type = config::DOMAIN_SYNC_COMMITTEE.to_vec();
			// Domains are used for for seeds, for signatures, and for selecting aggregators.
			let domain = Self::compute_domain(domain_type, fork_version, validators_root)?;
			// Hash tree root of SigningData - object root + domain
			let signing_root = Self::compute_signing_root(header, domain)?;
			Ok(signing_root)
		}
	}
}
