// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Provides glue code over the scheduler and inclusion modules, and accepting
//! one inherent per block that can include new para candidates and bitfields.
//!
//! Unlike other modules in this crate, it does not need to be initialized by the initializer,
//! as it has no initialization logic and its finalization logic depends only on the details of
//! this module.

use crate::{
	configuration,
	disputes::DisputesHandler,
	inclusion::{self, CandidateCheckContext},
	initializer,
	metrics::METRICS,
	paras,
	scheduler::{self, FreedReason},
	shared::{self, AllowedRelayParentsTracker},
	ParaId,
};
use bitvec::prelude::BitVec;
use frame_support::{
	defensive,
	dispatch::{DispatchErrorWithPostInfo, PostDispatchInfo},
	inherent::{InherentData, InherentIdentifier, MakeFatalError, ProvideInherent},
	pallet_prelude::*,
	traits::Randomness,
};
use frame_system::pallet_prelude::*;
use pallet_babe::{self, ParentBlockRandomness};
use primitives::{
	effective_minimum_backing_votes, node_features::FeatureIndex, BackedCandidate, CandidateHash,
	CandidateReceipt, CheckedDisputeStatementSet, CheckedMultiDisputeStatementSet, CoreIndex,
	DisputeStatementSet, HeadData, InherentData as ParachainsInherentData,
	MultiDisputeStatementSet, ScrapedOnChainVotes, SessionIndex, SignedAvailabilityBitfields,
	SigningContext, UncheckedSignedAvailabilityBitfield, UncheckedSignedAvailabilityBitfields,
	ValidatorId, ValidatorIndex, ValidityAttestation, PARACHAINS_INHERENT_IDENTIFIER,
};
use rand::{seq::SliceRandom, SeedableRng};
use scale_info::TypeInfo;
use sp_runtime::traits::{Header as HeaderT, One};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	prelude::*,
	vec::Vec,
};

mod misc;
mod weights;

use self::weights::checked_multi_dispute_statement_sets_weight;
pub use self::{
	misc::{IndexedRetain, IsSortedBy},
	weights::{
		backed_candidate_weight, backed_candidates_weight, dispute_statement_set_weight,
		multi_dispute_statement_sets_weight, paras_inherent_total_weight, signed_bitfield_weight,
		signed_bitfields_weight, TestWeightInfo, WeightInfo,
	},
};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "runtime::inclusion-inherent";

/// A bitfield concerning concluded disputes for candidates
/// associated to the core index equivalent to the bit position.
#[derive(Default, PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub(crate) struct DisputedBitfield(pub(crate) BitVec<u8, bitvec::order::Lsb0>);

impl From<BitVec<u8, bitvec::order::Lsb0>> for DisputedBitfield {
	fn from(inner: BitVec<u8, bitvec::order::Lsb0>) -> Self {
		Self(inner)
	}
}

#[cfg(test)]
impl DisputedBitfield {
	/// Create a new bitfield, where each bit is set to `false`.
	pub fn zeros(n: usize) -> Self {
		Self::from(BitVec::<u8, bitvec::order::Lsb0>::repeat(false, n))
	}
}

/// The context in which the inherent data is checked or processed.
#[derive(PartialEq)]
pub enum ProcessInherentDataContext {
	/// Enables filtering/limits weight of inherent up to maximum block weight.
	/// Invariant: InherentWeight <= BlockWeight.
	ProvideInherent,
	/// Checks the InherentWeight invariant.
	Enter,
}
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config:
		inclusion::Config + scheduler::Config + initializer::Config + pallet_babe::Config
	{
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Inclusion inherent called more than once per block.
		TooManyInclusionInherents,
		/// The hash of the submitted parent header doesn't correspond to the saved block hash of
		/// the parent.
		InvalidParentHeader,
		/// The data given to the inherent will result in an overweight block.
		InherentOverweight,
		/// A candidate was filtered during inherent execution. This should have only been done
		/// during creation.
		CandidatesFilteredDuringExecution,
		/// Too many candidates supplied.
		UnscheduledCandidate,
	}

	/// Whether the paras inherent was included within this block.
	///
	/// The `Option<()>` is effectively a `bool`, but it never hits storage in the `None` variant
	/// due to the guarantees of FRAME's storage APIs.
	///
	/// If this is `None` at the end of the block, we panic and render the block invalid.
	#[pallet::storage]
	pub(crate) type Included<T> = StorageValue<_, ()>;

	/// Scraped on chain data for extracting resolved disputes as well as backing votes.
	#[pallet::storage]
	#[pallet::getter(fn on_chain_votes)]
	pub(crate) type OnChainVotes<T: Config> = StorageValue<_, ScrapedOnChainVotes<T::Hash>>;

	/// Update the disputes statements set part of the on-chain votes.
	pub(crate) fn set_scrapable_on_chain_disputes<T: Config>(
		session: SessionIndex,
		checked_disputes: CheckedMultiDisputeStatementSet,
	) {
		crate::paras_inherent::OnChainVotes::<T>::mutate(move |value| {
			let disputes =
				checked_disputes.into_iter().map(DisputeStatementSet::from).collect::<Vec<_>>();
			let backing_validators_per_candidate = match value.take() {
				Some(v) => v.backing_validators_per_candidate,
				None => Vec::new(),
			};
			*value = Some(ScrapedOnChainVotes::<T::Hash> {
				backing_validators_per_candidate,
				disputes,
				session,
			});
		})
	}

	/// Update the backing votes including part of the on-chain votes.
	pub(crate) fn set_scrapable_on_chain_backings<T: Config>(
		session: SessionIndex,
		backing_validators_per_candidate: Vec<(
			CandidateReceipt<T::Hash>,
			Vec<(ValidatorIndex, ValidityAttestation)>,
		)>,
	) {
		crate::paras_inherent::OnChainVotes::<T>::mutate(move |value| {
			let disputes = match value.take() {
				Some(v) => v.disputes,
				None => MultiDisputeStatementSet::default(),
			};
			*value = Some(ScrapedOnChainVotes::<T::Hash> {
				backing_validators_per_candidate,
				disputes,
				session,
			});
		})
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads_writes(1, 1) // in `on_finalize`.
		}

		fn on_finalize(_: BlockNumberFor<T>) {
			if Included::<T>::take().is_none() {
				panic!("Bitfields and heads must be included every block");
			}
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier = PARACHAINS_INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let inherent_data = Self::create_inherent_inner(data)?;

			Some(Call::enter { data: inherent_data })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::enter { .. })
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Enter the paras inherent. This will process bitfields and backed candidates.
		#[pallet::call_index(0)]
		#[pallet::weight((
			paras_inherent_total_weight::<T>(
				data.backed_candidates.as_slice(),
				&data.bitfields,
				&data.disputes,
			),
			DispatchClass::Mandatory,
		))]
		pub fn enter(
			origin: OriginFor<T>,
			data: ParachainsInherentData<HeaderFor<T>>,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			ensure!(!Included::<T>::exists(), Error::<T>::TooManyInclusionInherents);
			Included::<T>::set(Some(()));

			Self::process_inherent_data(data, ProcessInherentDataContext::Enter)
				.map(|(_processed, post_info)| post_info)
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Create the `ParachainsInherentData` that gets passed to [`Self::enter`] in
	/// [`Self::create_inherent`]. This code is pulled out of [`Self::create_inherent`] so it can be
	/// unit tested.
	fn create_inherent_inner(data: &InherentData) -> Option<ParachainsInherentData<HeaderFor<T>>> {
		let parachains_inherent_data = match data.get_data(&Self::INHERENT_IDENTIFIER) {
			Ok(Some(d)) => d,
			Ok(None) => return None,
			Err(_) => {
				log::warn!(target: LOG_TARGET, "ParachainsInherentData failed to decode");
				return None
			},
		};
		match Self::process_inherent_data(
			parachains_inherent_data,
			ProcessInherentDataContext::ProvideInherent,
		) {
			Ok((processed, _)) => Some(processed),
			Err(err) => {
				log::warn!(target: LOG_TARGET, "Processing inherent data failed: {:?}", err);
				None
			},
		}
	}

	/// Process inherent data.
	///
	/// The given inherent data is processed and state is altered accordingly. If any data could
	/// not be applied (inconsistencies, weight limit, ...) it is removed.
	///
	/// When called from `create_inherent` the `context` must be set to
	/// `ProcessInherentDataContext::ProvideInherent` so it guarantees the invariant that inherent
	/// is not overweight.
	/// It is **mandatory** that calls from `enter` set `context` to
	/// `ProcessInherentDataContext::Enter` to ensure the weight invariant is checked.
	///
	/// Returns: Result containing processed inherent data and weight, the processed inherent would
	/// consume.
	fn process_inherent_data(
		data: ParachainsInherentData<HeaderFor<T>>,
		context: ProcessInherentDataContext,
	) -> sp_std::result::Result<
		(ParachainsInherentData<HeaderFor<T>>, PostDispatchInfo),
		DispatchErrorWithPostInfo,
	> {
		#[cfg(feature = "runtime-metrics")]
		sp_io::init_tracing();

		let ParachainsInherentData {
			mut bitfields,
			mut backed_candidates,
			parent_header,
			mut disputes,
		} = data;

		log::debug!(
			target: LOG_TARGET,
			"[process_inherent_data] bitfields.len(): {}, backed_candidates.len(): {}, disputes.len() {}",
			bitfields.len(),
			backed_candidates.len(),
			disputes.len()
		);

		let parent_hash = <frame_system::Pallet<T>>::parent_hash();

		ensure!(
			parent_header.hash().as_ref() == parent_hash.as_ref(),
			Error::<T>::InvalidParentHeader,
		);

		let now = <frame_system::Pallet<T>>::block_number();
		let config = <configuration::Pallet<T>>::config();

		// Before anything else, update the allowed relay-parents.
		{
			let parent_number = now - One::one();
			let parent_storage_root = *parent_header.state_root();

			shared::AllowedRelayParents::<T>::mutate(|tracker| {
				tracker.update(
					parent_hash,
					parent_storage_root,
					parent_number,
					config.async_backing_params.allowed_ancestry_len,
				);
			});
		}
		let allowed_relay_parents = <shared::Pallet<T>>::allowed_relay_parents();

		let candidates_weight = backed_candidates_weight::<T>(&backed_candidates);
		let bitfields_weight = signed_bitfields_weight::<T>(&bitfields);
		let disputes_weight = multi_dispute_statement_sets_weight::<T>(&disputes);

		// Weight before filtering/sanitization
		let all_weight_before = candidates_weight + bitfields_weight + disputes_weight;

		METRICS.on_before_filter(all_weight_before.ref_time());
		log::debug!(target: LOG_TARGET, "Size before filter: {}, candidates + bitfields: {}, disputes: {}", all_weight_before.proof_size(), candidates_weight.proof_size() + bitfields_weight.proof_size(), disputes_weight.proof_size());
		log::debug!(target: LOG_TARGET, "Time weight before filter: {}, candidates + bitfields: {}, disputes: {}", all_weight_before.ref_time(), candidates_weight.ref_time() + bitfields_weight.ref_time(), disputes_weight.ref_time());

		let current_session = <shared::Pallet<T>>::session_index();
		let expected_bits = <scheduler::Pallet<T>>::availability_cores().len();
		let validator_public = shared::Pallet::<T>::active_validator_keys();

		// We are assuming (incorrectly) to have all the weight (for the mandatory class or even
		// full block) available to us. This can lead to slightly overweight blocks, which still
		// works as the dispatch class for `enter` is `Mandatory`. By using the `Mandatory`
		// dispatch class, the upper layers impose no limit on the weight of this inherent, instead
		// we limit ourselves and make sure to stay within reasonable bounds. It might make sense
		// to subtract BlockWeights::base_block to reduce chances of becoming overweight.
		let max_block_weight = {
			let dispatch_class = DispatchClass::Mandatory;
			let max_block_weight_full = <T as frame_system::Config>::BlockWeights::get();
			log::debug!(target: LOG_TARGET, "Max block weight: {}", max_block_weight_full.max_block);
			// Get max block weight for the mandatory class if defined, otherwise total max weight
			// of the block.
			let max_weight = max_block_weight_full
				.per_class
				.get(dispatch_class)
				.max_total
				.unwrap_or(max_block_weight_full.max_block);
			log::debug!(target: LOG_TARGET, "Used max block time weight: {}", max_weight);

			let max_block_size_full = <T as frame_system::Config>::BlockLength::get();
			let max_block_size = max_block_size_full.max.get(dispatch_class);
			log::debug!(target: LOG_TARGET, "Used max block size: {}", max_block_size);

			// Adjust proof size to max block size as we are tracking tx size.
			max_weight.set_proof_size(*max_block_size as u64)
		};
		log::debug!(target: LOG_TARGET, "Used max block weight: {}", max_block_weight);

		let entropy = compute_entropy::<T>(parent_hash);
		let mut rng = rand_chacha::ChaChaRng::from_seed(entropy.into());

		// Filter out duplicates and continue.
		if let Err(()) = T::DisputesHandler::deduplicate_and_sort_dispute_data(&mut disputes) {
			log::debug!(target: LOG_TARGET, "Found duplicate statement sets, retaining the first");
		}

		let post_conclusion_acceptance_period = config.dispute_post_conclusion_acceptance_period;

		let dispute_statement_set_valid = move |set: DisputeStatementSet| {
			T::DisputesHandler::filter_dispute_data(set, post_conclusion_acceptance_period)
		};

		// Limit the disputes first, since the following statements depend on the votes include
		// here.
		let (checked_disputes_sets, checked_disputes_sets_consumed_weight) =
			limit_and_sanitize_disputes::<T, _>(
				disputes,
				dispute_statement_set_valid,
				max_block_weight,
			);

		let all_weight_after = if context == ProcessInherentDataContext::ProvideInherent {
			// Assure the maximum block weight is adhered, by limiting bitfields and backed
			// candidates. Dispute statement sets were already limited before.
			let non_disputes_weight = apply_weight_limit::<T>(
				&mut backed_candidates,
				&mut bitfields,
				max_block_weight.saturating_sub(checked_disputes_sets_consumed_weight),
				&mut rng,
			);

			let all_weight_after =
				non_disputes_weight.saturating_add(checked_disputes_sets_consumed_weight);

			METRICS.on_after_filter(all_weight_after.ref_time());
			log::debug!(
			target: LOG_TARGET,
			"[process_inherent_data] after filter: bitfields.len(): {}, backed_candidates.len(): {}, checked_disputes_sets.len() {}",
			bitfields.len(),
			backed_candidates.len(),
			checked_disputes_sets.len()
			);
			log::debug!(target: LOG_TARGET, "Size after filter: {}, candidates + bitfields: {}, disputes: {}", all_weight_after.proof_size(), non_disputes_weight.proof_size(), checked_disputes_sets_consumed_weight.proof_size());
			log::debug!(target: LOG_TARGET, "Time weight after filter: {}, candidates + bitfields: {}, disputes: {}", all_weight_after.ref_time(), non_disputes_weight.ref_time(), checked_disputes_sets_consumed_weight.ref_time());

			if all_weight_after.any_gt(max_block_weight) {
				log::warn!(target: LOG_TARGET, "Post weight limiting weight is still too large, time: {}, size: {}", all_weight_after.ref_time(), all_weight_after.proof_size());
			}
			all_weight_after
		} else {
			// This check is performed in the context of block execution. Ensures inherent weight
			// invariants guaranteed by `create_inherent_data` for block authorship.
			if all_weight_before.any_gt(max_block_weight) {
				log::error!(
					"Overweight para inherent data reached the runtime {:?}: {} > {}",
					parent_hash,
					all_weight_before,
					max_block_weight
				);
			}

			ensure!(all_weight_before.all_lte(max_block_weight), Error::<T>::InherentOverweight);
			all_weight_before
		};

		// Note that `process_checked_multi_dispute_data` will iterate and import each
		// dispute; so the input here must be reasonably bounded,
		// which is guaranteed by the checks and weight limitation above.
		// We don't care about fresh or not disputes
		// this writes them to storage, so let's query it via those means
		// if this fails for whatever reason, that's ok.
		if let Err(e) =
			T::DisputesHandler::process_checked_multi_dispute_data(&checked_disputes_sets)
		{
			log::warn!(target: LOG_TARGET, "MultiDisputesData failed to update: {:?}", e);
		};
		METRICS.on_disputes_imported(checked_disputes_sets.len() as u64);

		set_scrapable_on_chain_disputes::<T>(current_session, checked_disputes_sets.clone());

		if T::DisputesHandler::is_frozen() {
			// Relay chain freeze, at this point we will not include any parachain blocks.
			METRICS.on_relay_chain_freeze();

			let disputes = checked_disputes_sets
				.into_iter()
				.map(|checked| checked.into())
				.collect::<Vec<_>>();
			let processed = ParachainsInherentData {
				bitfields: Vec::new(),
				backed_candidates: Vec::new(),
				disputes,
				parent_header,
			};

			// The relay chain we are currently on is invalid. Proceed no further on parachains.
			return Ok((processed, Some(checked_disputes_sets_consumed_weight).into()))
		}

		// Contains the disputes that are concluded in the current session only,
		// since these are the only ones that are relevant for the occupied cores
		// and lightens the load on `free_disputed` significantly.
		// Cores can't be occupied with candidates of the previous sessions, and only
		// things with new votes can have just concluded. We only need to collect
		// cores with disputes that conclude just now, because disputes that
		// concluded longer ago have already had any corresponding cores cleaned up.
		let current_concluded_invalid_disputes = checked_disputes_sets
			.iter()
			.map(AsRef::as_ref)
			.filter(|dss| dss.session == current_session)
			.map(|dss| (dss.session, dss.candidate_hash))
			.filter(|(session, candidate)| {
				<T>::DisputesHandler::concluded_invalid(*session, *candidate)
			})
			.map(|(_session, candidate)| candidate)
			.collect::<BTreeSet<CandidateHash>>();

		// Get the cores freed as a result of concluded invalid candidates.
		let (freed_disputed, concluded_invalid_hashes): (Vec<CoreIndex>, BTreeSet<CandidateHash>) =
			<inclusion::Pallet<T>>::free_disputed(&current_concluded_invalid_disputes)
				.into_iter()
				.unzip();

		// Create a bit index from the set of core indices where each index corresponds to
		// a core index that was freed due to a dispute.
		//
		// I.e. 010100 would indicate, the candidates on Core 1 and 3 would be disputed.
		let disputed_bitfield = create_disputed_bitfield(expected_bits, freed_disputed.iter());

		let bitfields = sanitize_bitfields::<T>(
			bitfields,
			disputed_bitfield,
			expected_bits,
			parent_hash,
			current_session,
			&validator_public[..],
		);
		METRICS.on_bitfields_processed(bitfields.len() as u64);

		// Process new availability bitfields, yielding any availability cores whose
		// work has now concluded.
		let freed_concluded =
			<inclusion::Pallet<T>>::update_pending_availability_and_get_freed_cores(
				&validator_public[..],
				bitfields.clone(),
			);

		// Inform the disputes module of all included candidates.
		for (_, candidate_hash) in &freed_concluded {
			T::DisputesHandler::note_included(current_session, *candidate_hash, now);
		}

		METRICS.on_candidates_included(freed_concluded.len() as u64);

		// Get the timed out candidates
		let freed_timeout = if <scheduler::Pallet<T>>::availability_timeout_check_required() {
			<inclusion::Pallet<T>>::free_timedout()
		} else {
			Vec::new()
		};

		if !freed_timeout.is_empty() {
			log::debug!(target: LOG_TARGET, "Evicted timed out cores: {:?}", freed_timeout);
		}

		// We'll schedule paras again, given freed cores, and reasons for freeing.
		let freed = freed_concluded
			.into_iter()
			.map(|(c, _hash)| (c, FreedReason::Concluded))
			.chain(freed_disputed.into_iter().map(|core| (core, FreedReason::Concluded)))
			.chain(freed_timeout.into_iter().map(|c| (c, FreedReason::TimedOut)))
			.collect::<BTreeMap<CoreIndex, FreedReason>>();
		<scheduler::Pallet<T>>::free_cores_and_fill_claimqueue(freed, now);

		METRICS.on_candidates_processed_total(backed_candidates.len() as u64);

		let core_index_enabled = configuration::Pallet::<T>::config()
			.node_features
			.get(FeatureIndex::ElasticScalingMVP as usize)
			.map(|b| *b)
			.unwrap_or(false);

		let mut scheduled: BTreeMap<ParaId, BTreeSet<CoreIndex>> = BTreeMap::new();
		let mut total_scheduled_cores = 0;

		for (core_idx, para_id) in <scheduler::Pallet<T>>::scheduled_paras() {
			total_scheduled_cores += 1;
			scheduled.entry(para_id).or_default().insert(core_idx);
		}

		let initial_candidate_count = backed_candidates.len();
		let backed_candidates_with_core = sanitize_backed_candidates::<T>(
			backed_candidates,
			&allowed_relay_parents,
			concluded_invalid_hashes,
			scheduled,
			core_index_enabled,
		);
		let count = count_backed_candidates(&backed_candidates_with_core);

		ensure!(count <= total_scheduled_cores, Error::<T>::UnscheduledCandidate);

		METRICS.on_candidates_sanitized(count as u64);

		// In `Enter` context (invoked during execution) no more candidates should be filtered,
		// because they have already been filtered during `ProvideInherent` context. Abort in such
		// cases.
		if context == ProcessInherentDataContext::Enter {
			ensure!(
				initial_candidate_count == count,
				Error::<T>::CandidatesFilteredDuringExecution
			);
		}

		// Process backed candidates according to scheduled cores.
		let inclusion::ProcessedCandidates::<<HeaderFor<T> as HeaderT>::Hash> {
			core_indices: occupied,
			candidate_receipt_with_backing_validator_indices,
		} = <inclusion::Pallet<T>>::process_candidates(
			&allowed_relay_parents,
			&backed_candidates_with_core,
			<scheduler::Pallet<T>>::group_validators,
			core_index_enabled,
		)?;
		// Note which of the scheduled cores were actually occupied by a backed candidate.
		<scheduler::Pallet<T>>::occupied(occupied.into_iter().map(|e| (e.0, e.1)).collect());

		set_scrapable_on_chain_backings::<T>(
			current_session,
			candidate_receipt_with_backing_validator_indices,
		);

		let disputes = checked_disputes_sets
			.into_iter()
			.map(|checked| checked.into())
			.collect::<Vec<_>>();

		let bitfields = bitfields.into_iter().map(|v| v.into_unchecked()).collect();

		let processed = ParachainsInherentData {
			bitfields,
			backed_candidates: backed_candidates_with_core.into_iter().fold(
				Vec::with_capacity(count),
				|mut acc, (_id, candidates)| {
					acc.extend(candidates.into_iter().map(|(c, _)| c));
					acc
				},
			),
			disputes,
			parent_header,
		};
		Ok((processed, Some(all_weight_after).into()))
	}
}

/// Derive a bitfield from dispute
pub(super) fn create_disputed_bitfield<'a, I>(
	expected_bits: usize,
	freed_cores: I,
) -> DisputedBitfield
where
	I: 'a + IntoIterator<Item = &'a CoreIndex>,
{
	let mut bitvec = BitVec::repeat(false, expected_bits);
	for core_idx in freed_cores {
		let core_idx = core_idx.0 as usize;
		if core_idx < expected_bits {
			bitvec.set(core_idx, true);
		}
	}
	DisputedBitfield::from(bitvec)
}

/// Select a random subset, with preference for certain indices.
///
/// Adds random items to the set until all candidates
/// are tried or the remaining weight is depleted.
///
/// Returns the weight of all selected items from `selectables`
/// as well as their indices in ascending order.
fn random_sel<X, F: Fn(&X) -> Weight>(
	rng: &mut rand_chacha::ChaChaRng,
	selectables: &[X],
	mut preferred_indices: Vec<usize>,
	weight_fn: F,
	weight_limit: Weight,
) -> (Weight, Vec<usize>) {
	if selectables.is_empty() {
		return (Weight::zero(), Vec::new())
	}
	// all indices that are not part of the preferred set
	let mut indices = (0..selectables.len())
		.into_iter()
		.filter(|idx| !preferred_indices.contains(idx))
		.collect::<Vec<_>>();
	let mut picked_indices = Vec::with_capacity(selectables.len().saturating_sub(1));

	let mut weight_acc = Weight::zero();

	preferred_indices.shuffle(rng);
	for preferred_idx in preferred_indices {
		// preferred indices originate from outside
		if let Some(item) = selectables.get(preferred_idx) {
			let updated = weight_acc.saturating_add(weight_fn(item));
			if updated.any_gt(weight_limit) {
				continue
			}
			weight_acc = updated;
			picked_indices.push(preferred_idx);
		}
	}

	indices.shuffle(rng);
	for idx in indices {
		let item = &selectables[idx];
		let updated = weight_acc.saturating_add(weight_fn(item));

		if updated.any_gt(weight_limit) {
			continue
		}
		weight_acc = updated;

		picked_indices.push(idx);
	}

	// sorting indices, so the ordering is retained
	// unstable sorting is fine, since there are no duplicates in indices
	// and even if there were, they don't have an identity
	picked_indices.sort_unstable();
	(weight_acc, picked_indices)
}

/// Considers an upper threshold that the inherent data must not exceed.
///
/// If there is sufficient space, all bitfields and all candidates
/// will be included.
///
/// Otherwise tries to include all disputes, and then tries to fill the remaining space with
/// bitfields and then candidates.
///
/// The selection process is random. For candidates, there is an exception for code upgrades as they
/// are preferred. And for disputes, local and older disputes are preferred (see
/// `limit_and_sanitize_disputes`). for backed candidates, since with a increasing number of
/// parachains their chances of inclusion become slim. All backed candidates  are checked
/// beforehand in `fn create_inherent_inner` which guarantees sanity.
///
/// Assumes disputes are already filtered by the time this is called.
///
/// Returns the total weight consumed by `bitfields` and `candidates`.
pub(crate) fn apply_weight_limit<T: Config + inclusion::Config>(
	candidates: &mut Vec<BackedCandidate<<T>::Hash>>,
	bitfields: &mut UncheckedSignedAvailabilityBitfields,
	max_consumable_weight: Weight,
	rng: &mut rand_chacha::ChaChaRng,
) -> Weight {
	let total_candidates_weight = backed_candidates_weight::<T>(candidates.as_slice());

	let total_bitfields_weight = signed_bitfields_weight::<T>(&bitfields);

	let total = total_bitfields_weight.saturating_add(total_candidates_weight);

	// candidates + bitfields fit into the block
	if max_consumable_weight.all_gte(total) {
		return total
	}

	// Invariant: block author provides candidate in the order in which they form a chain
	// wrt elastic scaling. If the invariant is broken, we'd fail later when filtering candidates
	// which are unchained.

	let mut chained_candidates: Vec<Vec<_>> = Vec::new();
	let mut current_para_id = None;

	for candidate in sp_std::mem::take(candidates).into_iter() {
		let candidate_para_id = candidate.descriptor().para_id;
		if Some(candidate_para_id) == current_para_id {
			let chain = chained_candidates
				.last_mut()
				.expect("if the current_para_id is Some, then vec is not empty; qed");
			chain.push(candidate);
		} else {
			current_para_id = Some(candidate_para_id);
			chained_candidates.push(vec![candidate]);
		}
	}

	// Elastic scaling: we prefer chains that have a code upgrade among the candidates,
	// as the candidates containing the upgrade tend to be large and hence stand no chance to
	// be picked late while maintaining the weight bounds.
	//
	// Limitations: For simplicity if total weight of a chain of candidates is larger than
	// the remaining weight, the chain will still not be included while it could still be possible
	// to include part of that chain.
	let preferred_chain_indices = chained_candidates
		.iter()
		.enumerate()
		.filter_map(|(idx, candidates)| {
			// Check if any of the candidate in chain contains a code upgrade.
			if candidates
				.iter()
				.any(|candidate| candidate.candidate().commitments.new_validation_code.is_some())
			{
				Some(idx)
			} else {
				None
			}
		})
		.collect::<Vec<usize>>();

	// There is weight remaining to be consumed by a subset of chained candidates
	// which are going to be picked now.
	if let Some(max_consumable_by_candidates) =
		max_consumable_weight.checked_sub(&total_bitfields_weight)
	{
		let (acc_candidate_weight, chained_indices) =
			random_sel::<Vec<BackedCandidate<<T as frame_system::Config>::Hash>>, _>(
				rng,
				&chained_candidates,
				preferred_chain_indices,
				|candidates| backed_candidates_weight::<T>(&candidates),
				max_consumable_by_candidates,
			);
		log::debug!(target: LOG_TARGET, "Indices Candidates: {:?}, size: {}", chained_indices, candidates.len());
		chained_candidates
			.indexed_retain(|idx, _backed_candidates| chained_indices.binary_search(&idx).is_ok());
		// pick all bitfields, and
		// fill the remaining space with candidates
		let total_consumed = acc_candidate_weight.saturating_add(total_bitfields_weight);

		*candidates = chained_candidates.into_iter().flatten().collect::<Vec<_>>();

		return total_consumed
	}

	candidates.clear();

	// insufficient space for even the bitfields alone, so only try to fit as many of those
	// into the block and skip the candidates entirely
	let (total_consumed, indices) = random_sel::<UncheckedSignedAvailabilityBitfield, _>(
		rng,
		&bitfields,
		vec![],
		|bitfield| signed_bitfield_weight::<T>(&bitfield),
		max_consumable_weight,
	);
	log::debug!(target: LOG_TARGET, "Indices Bitfields: {:?}, size: {}", indices, bitfields.len());

	bitfields.indexed_retain(|idx, _bitfield| indices.binary_search(&idx).is_ok());

	total_consumed
}

/// Filter bitfields based on freed core indices, validity, and other sanity checks.
///
/// Do sanity checks on the bitfields:
///
///  1. no more than one bitfield per validator
///  2. bitfields are ascending by validator index.
///  3. each bitfield has exactly `expected_bits`
///  4. signature is valid
///  5. remove any disputed core indices
///
/// If any of those is not passed, the bitfield is dropped.
pub(crate) fn sanitize_bitfields<T: crate::inclusion::Config>(
	unchecked_bitfields: UncheckedSignedAvailabilityBitfields,
	disputed_bitfield: DisputedBitfield,
	expected_bits: usize,
	parent_hash: T::Hash,
	session_index: SessionIndex,
	validators: &[ValidatorId],
) -> SignedAvailabilityBitfields {
	let mut bitfields = Vec::with_capacity(unchecked_bitfields.len());

	let mut last_index: Option<ValidatorIndex> = None;

	if disputed_bitfield.0.len() != expected_bits {
		// This is a system logic error that should never occur, but we want to handle it gracefully
		// so we just drop all bitfields
		log::error!(target: LOG_TARGET, "BUG: disputed_bitfield != expected_bits");
		return vec![]
	}

	let all_zeros = BitVec::<u8, bitvec::order::Lsb0>::repeat(false, expected_bits);
	let signing_context = SigningContext { parent_hash, session_index };
	for unchecked_bitfield in unchecked_bitfields {
		// Find and skip invalid bitfields.
		if unchecked_bitfield.unchecked_payload().0.len() != expected_bits {
			log::trace!(
				target: LOG_TARGET,
				"bad bitfield length: {} != {:?}",
				unchecked_bitfield.unchecked_payload().0.len(),
				expected_bits,
			);
			continue
		}

		if unchecked_bitfield.unchecked_payload().0.clone() & disputed_bitfield.0.clone() !=
			all_zeros
		{
			log::trace!(
				target: LOG_TARGET,
				"bitfield contains disputed cores: {:?}",
				unchecked_bitfield.unchecked_payload().0.clone() & disputed_bitfield.0.clone()
			);
			continue
		}

		let validator_index = unchecked_bitfield.unchecked_validator_index();

		if !last_index.map_or(true, |last_index: ValidatorIndex| last_index < validator_index) {
			log::trace!(
				target: LOG_TARGET,
				"bitfield validator index is not greater than last: !({:?} < {})",
				last_index.as_ref().map(|x| x.0),
				validator_index.0
			);
			continue
		}

		if unchecked_bitfield.unchecked_validator_index().0 as usize >= validators.len() {
			log::trace!(
				target: LOG_TARGET,
				"bitfield validator index is out of bounds: {} >= {}",
				validator_index.0,
				validators.len(),
			);
			continue
		}

		let validator_public = &validators[validator_index.0 as usize];

		// Validate bitfield signature.
		if let Ok(signed_bitfield) =
			unchecked_bitfield.try_into_checked(&signing_context, validator_public)
		{
			bitfields.push(signed_bitfield);
			METRICS.on_valid_bitfield_signature();
		} else {
			log::warn!(target: LOG_TARGET, "Invalid bitfield signature");
			METRICS.on_invalid_bitfield_signature();
		};

		last_index = Some(validator_index);
	}
	bitfields
}

/// Performs various filtering on the backed candidates inherent data.
/// Must maintain the invariant that the returned candidate collection contains the candidates
/// sorted in dependency order for each para. When doing any filtering, we must therefore drop any
/// subsequent candidates after the filtered one.
///
/// Filter out:
/// 1. any candidates which don't form a chain with the other candidates of the paraid (even if they
///    do form a chain but are not in the right order).
/// 2. any candidates that have a concluded invalid dispute or who are descendants of a concluded
///    invalid candidate.
/// 3. any unscheduled candidates, as well as candidates whose paraid has multiple cores assigned
///    but have no injected core index.
/// 4. all backing votes from disabled validators
/// 5. any candidates that end up with less than `effective_minimum_backing_votes` backing votes
///
/// Returns the scheduled
/// backed candidates which passed filtering, mapped by para id and in the right dependency order.
fn sanitize_backed_candidates<T: crate::inclusion::Config>(
	backed_candidates: Vec<BackedCandidate<T::Hash>>,
	allowed_relay_parents: &AllowedRelayParentsTracker<T::Hash, BlockNumberFor<T>>,
	concluded_invalid_with_descendants: BTreeSet<CandidateHash>,
	scheduled: BTreeMap<ParaId, BTreeSet<CoreIndex>>,
	core_index_enabled: bool,
) -> BTreeMap<ParaId, Vec<(BackedCandidate<T::Hash>, CoreIndex)>> {
	// Map the candidates to the right paraids, while making sure that the order between candidates
	// of the same para is preserved.
	let mut candidates_per_para: BTreeMap<ParaId, Vec<_>> = BTreeMap::new();
	for candidate in backed_candidates {
		candidates_per_para
			.entry(candidate.descriptor().para_id)
			.or_default()
			.push(candidate);
	}

	// Check that candidates pertaining to the same para form a chain. Drop the ones that
	// don't, along with the rest of candidates which follow them in the input vector.
	filter_unchained_candidates::<T>(&mut candidates_per_para, allowed_relay_parents);

	// Remove any candidates that were concluded invalid or who are descendants of concluded invalid
	// candidates (along with their descendants).
	retain_candidates::<T, _, _>(&mut candidates_per_para, |_, candidate| {
		let keep = !concluded_invalid_with_descendants.contains(&candidate.candidate().hash());

		if !keep {
			log::debug!(
				target: LOG_TARGET,
				"Found backed candidate {:?} which was concluded invalid or is a descendant of a concluded invalid candidate, for paraid {:?}.",
				candidate.candidate().hash(),
				candidate.descriptor().para_id
			);
		}
		keep
	});

	// Map candidates to scheduled cores. Filter out any unscheduled candidates along with their
	// descendants.
	let mut backed_candidates_with_core = map_candidates_to_cores::<T>(
		&allowed_relay_parents,
		scheduled,
		core_index_enabled,
		candidates_per_para,
	);

	// Filter out backing statements from disabled validators. If by that we render a candidate with
	// less backing votes than required, filter that candidate also. As all the other filtering
	// operations above, we drop the descendants of the dropped candidates also.
	filter_backed_statements_from_disabled_validators::<T>(
		&mut backed_candidates_with_core,
		&allowed_relay_parents,
		core_index_enabled,
	);

	backed_candidates_with_core
}

fn count_backed_candidates<B>(backed_candidates: &BTreeMap<ParaId, Vec<B>>) -> usize {
	backed_candidates.iter().fold(0, |mut count, (_id, candidates)| {
		count += candidates.len();
		count
	})
}

/// Derive entropy from babe provided per block randomness.
///
/// In the odd case none is available, uses the `parent_hash` and
/// a const value, while emitting a warning.
fn compute_entropy<T: Config>(parent_hash: T::Hash) -> [u8; 32] {
	const CANDIDATE_SEED_SUBJECT: [u8; 32] = *b"candidate-seed-selection-subject";
	// NOTE: this is slightly gameable since this randomness was already public
	// by the previous block, while for the block author this randomness was
	// known 2 epochs ago. it is marginally better than using the parent block
	// hash since it's harder to influence the VRF output than the block hash.
	let vrf_random = ParentBlockRandomness::<T>::random(&CANDIDATE_SEED_SUBJECT[..]).0;
	let mut entropy: [u8; 32] = CANDIDATE_SEED_SUBJECT;
	if let Some(vrf_random) = vrf_random {
		entropy.as_mut().copy_from_slice(vrf_random.as_ref());
	} else {
		// in case there is no VRF randomness present, we utilize the relay parent
		// as seed, it's better than a static value.
		log::warn!(target: LOG_TARGET, "ParentBlockRandomness did not provide entropy");
		entropy.as_mut().copy_from_slice(parent_hash.as_ref());
	}
	entropy
}

/// Limit disputes in place.
///
/// Assumes ordering of disputes, retains sorting of the statement.
///
/// Prime source of overload safety for dispute votes:
/// 1. Check accumulated weight does not exceed the maximum block weight.
/// 2. If exceeded:
///   1. Check validity of all dispute statements sequentially
/// 2. If not exceeded:
///   1. If weight is exceeded by locals, pick the older ones (lower indices) until the weight limit
///      is reached.
///
/// Returns the consumed weight amount, that is guaranteed to be less than the provided
/// `max_consumable_weight`.
fn limit_and_sanitize_disputes<
	T: Config,
	CheckValidityFn: FnMut(DisputeStatementSet) -> Option<CheckedDisputeStatementSet>,
>(
	disputes: MultiDisputeStatementSet,
	mut dispute_statement_set_valid: CheckValidityFn,
	max_consumable_weight: Weight,
) -> (Vec<CheckedDisputeStatementSet>, Weight) {
	// The total weight if all disputes would be included
	let disputes_weight = multi_dispute_statement_sets_weight::<T>(&disputes);

	if disputes_weight.any_gt(max_consumable_weight) {
		log::debug!(target: LOG_TARGET, "Above max consumable weight: {}/{}", disputes_weight, max_consumable_weight);
		let mut checked_acc = Vec::<CheckedDisputeStatementSet>::with_capacity(disputes.len());

		// Accumulated weight of all disputes picked, that passed the checks.
		let mut weight_acc = Weight::zero();

		// Select disputes in-order until the remaining weight is attained
		disputes.into_iter().for_each(|dss| {
			let dispute_weight = dispute_statement_set_weight::<T, &DisputeStatementSet>(&dss);
			let updated = weight_acc.saturating_add(dispute_weight);
			if max_consumable_weight.all_gte(updated) {
				// Always apply the weight. Invalid data cost processing time too:
				weight_acc = updated;
				if let Some(checked) = dispute_statement_set_valid(dss) {
					checked_acc.push(checked);
				}
			}
		});

		(checked_acc, weight_acc)
	} else {
		// Go through all of them, and just apply the filter, they would all fit
		let checked = disputes
			.into_iter()
			.filter_map(|dss| dispute_statement_set_valid(dss))
			.collect::<Vec<CheckedDisputeStatementSet>>();
		// some might have been filtered out, so re-calc the weight
		let checked_disputes_weight = checked_multi_dispute_statement_sets_weight::<T>(&checked);
		(checked, checked_disputes_weight)
	}
}

// Helper function for filtering candidates which don't pass the given predicate. When/if the first
// candidate which failes the predicate is found, all the other candidates that follow are dropped.
fn retain_candidates<
	T: inclusion::Config + paras::Config + inclusion::Config,
	F: FnMut(ParaId, &mut C) -> bool,
	C,
>(
	candidates_per_para: &mut BTreeMap<ParaId, Vec<C>>,
	mut pred: F,
) {
	for (para_id, candidates) in candidates_per_para.iter_mut() {
		let mut latest_valid_idx = None;

		for (idx, candidate) in candidates.iter_mut().enumerate() {
			if pred(*para_id, candidate) {
				// Found a valid candidate.
				latest_valid_idx = Some(idx);
			} else {
				break
			}
		}

		if let Some(latest_valid_idx) = latest_valid_idx {
			candidates.truncate(latest_valid_idx + 1);
		} else {
			candidates.clear();
		}
	}

	candidates_per_para.retain(|_, c| !c.is_empty());
}

// Filters statements from disabled validators in `BackedCandidate` and does a few more sanity
// checks.
fn filter_backed_statements_from_disabled_validators<
	T: shared::Config + scheduler::Config + inclusion::Config,
>(
	backed_candidates_with_core: &mut BTreeMap<
		ParaId,
		Vec<(BackedCandidate<<T as frame_system::Config>::Hash>, CoreIndex)>,
	>,
	allowed_relay_parents: &AllowedRelayParentsTracker<T::Hash, BlockNumberFor<T>>,
	core_index_enabled: bool,
) {
	let disabled_validators =
		BTreeSet::<_>::from_iter(shared::Pallet::<T>::disabled_validators().into_iter());

	if disabled_validators.is_empty() {
		// No disabled validators - nothing to do
		return
	}

	let minimum_backing_votes = configuration::Pallet::<T>::config().minimum_backing_votes;

	// Process all backed candidates. `validator_indices` in `BackedCandidates` are indices within
	// the validator group assigned to the parachain. To obtain this group we need:
	// 1. Core index assigned to the parachain which has produced the candidate
	// 2. The relay chain block number of the candidate
	retain_candidates::<T, _, _>(backed_candidates_with_core, |para_id, (bc, core_idx)| {
		let (validator_indices, maybe_core_index) =
			bc.validator_indices_and_core_index(core_index_enabled);
		let mut validator_indices = BitVec::<_>::from(validator_indices);

		// Get relay parent block number of the candidate. We need this to get the group index
		// assigned to this core at this block number
		let relay_parent_block_number =
			match allowed_relay_parents.acquire_info(bc.descriptor().relay_parent, None) {
				Some((_, block_num)) => block_num,
				None => {
					log::debug!(
						target: LOG_TARGET,
						"Relay parent {:?} for candidate is not in the allowed relay parents. Dropping the candidate.",
						bc.descriptor().relay_parent
					);
					return false
				},
			};

		// Get the group index for the core
		let group_idx = match <scheduler::Pallet<T>>::group_assigned_to_core(
			*core_idx,
			relay_parent_block_number + One::one(),
		) {
			Some(group_idx) => group_idx,
			None => {
				log::debug!(target: LOG_TARGET, "Can't get the group index for core idx {:?}. Dropping the candidate.", core_idx);
				return false
			},
		};

		// And finally get the validator group for this group index
		let validator_group = match <scheduler::Pallet<T>>::group_validators(group_idx) {
			Some(validator_group) => validator_group,
			None => {
				log::debug!(target: LOG_TARGET, "Can't get the validators from group {:?}. Dropping the candidate.", group_idx);
				return false
			},
		};

		// Bitmask with the disabled indices within the validator group
		let disabled_indices = BitVec::<u8, bitvec::order::Lsb0>::from_iter(
			validator_group.iter().map(|idx| disabled_validators.contains(idx)),
		);
		// The indices of statements from disabled validators in `BackedCandidate`. We have to drop
		// these.
		let indices_to_drop = disabled_indices.clone() & &validator_indices;
		// Apply the bitmask to drop the disabled validator from `validator_indices`
		validator_indices &= !disabled_indices;
		// Update the backed candidate
		bc.set_validator_indices_and_core_index(validator_indices, maybe_core_index);

		// Remove the corresponding votes from `validity_votes`
		for idx in indices_to_drop.iter_ones().rev() {
			bc.validity_votes_mut().remove(idx);
		}

		// By filtering votes we might render the candidate invalid and cause a failure in
		// [`process_candidates`]. To avoid this we have to perform a sanity check here. If there
		// are not enough backing votes after filtering we will remove the whole candidate.
		if bc.validity_votes().len() <
			effective_minimum_backing_votes(validator_group.len(), minimum_backing_votes)
		{
			log::debug!(
				target: LOG_TARGET,
				"Dropping candidate {:?} of paraid {:?} because it was left with too few backing votes after votes from disabled validators were filtered.",
				bc.candidate().hash(),
				para_id
			);

			return false
		}

		true
	});
}

// Check that candidates pertaining to the same para form a chain. Drop the ones that
// don't, along with the rest of candidates which follow them in the input vector.
// In the process, duplicated candidates will also be dropped (even if they form a valid cycle;
// cycles are not allowed if they entail backing duplicated candidates).
fn filter_unchained_candidates<T: inclusion::Config + paras::Config + inclusion::Config>(
	candidates: &mut BTreeMap<ParaId, Vec<BackedCandidate<T::Hash>>>,
	allowed_relay_parents: &AllowedRelayParentsTracker<T::Hash, BlockNumberFor<T>>,
) {
	let mut para_latest_head_data: BTreeMap<ParaId, HeadData> = BTreeMap::new();
	for para_id in candidates.keys() {
		let latest_head_data = match <inclusion::Pallet<T>>::para_latest_head_data(&para_id) {
			None => {
				defensive!("Latest included head data for paraid {:?} is None", para_id);
				continue
			},
			Some(latest_head_data) => latest_head_data,
		};
		para_latest_head_data.insert(*para_id, latest_head_data);
	}

	let mut para_visited_candidates: BTreeMap<ParaId, BTreeSet<CandidateHash>> = BTreeMap::new();

	retain_candidates::<T, _, _>(candidates, |para_id, candidate| {
		let Some(latest_head_data) = para_latest_head_data.get(&para_id) else { return false };
		let candidate_hash = candidate.candidate().hash();

		let visited_candidates =
			para_visited_candidates.entry(para_id).or_insert_with(|| BTreeSet::new());
		if visited_candidates.contains(&candidate_hash) {
			log::debug!(
				target: LOG_TARGET,
				"Found duplicate candidates for paraid {:?}. Dropping the candidates with hash {:?}",
				para_id,
				candidate_hash
			);

			// If we got a duplicate candidate, stop.
			return false
		} else {
			visited_candidates.insert(candidate_hash);
		}

		let prev_context = <paras::Pallet<T>>::para_most_recent_context(para_id);
		let check_ctx = CandidateCheckContext::<T>::new(prev_context);

		let res = match check_ctx.verify_backed_candidate(
			&allowed_relay_parents,
			candidate.candidate(),
			latest_head_data.clone(),
		) {
			Ok(_) => true,
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"Backed candidate verification for candidate {:?} of paraid {:?} failed with {:?}",
					candidate_hash,
					para_id,
					err
				);
				false
			},
		};

		if res {
			para_latest_head_data
				.insert(para_id, candidate.candidate().commitments.head_data.clone());
		}

		res
	});
}

/// Map candidates to scheduled cores.
/// If the para only has one scheduled core and one candidate supplied, map the candidate to the
/// single core. If the para has multiple cores scheduled, only map the candidates which have a
/// proper core injected. Filter out the rest.
/// Also returns whether or not we dropped any candidates.
/// When dropping a candidate of a para, we must drop all subsequent candidates from that para
/// (because they form a chain).
fn map_candidates_to_cores<T: configuration::Config + scheduler::Config + inclusion::Config>(
	allowed_relay_parents: &AllowedRelayParentsTracker<T::Hash, BlockNumberFor<T>>,
	mut scheduled: BTreeMap<ParaId, BTreeSet<CoreIndex>>,
	core_index_enabled: bool,
	candidates: BTreeMap<ParaId, Vec<BackedCandidate<T::Hash>>>,
) -> BTreeMap<ParaId, Vec<(BackedCandidate<T::Hash>, CoreIndex)>> {
	let mut backed_candidates_with_core = BTreeMap::new();

	for (para_id, backed_candidates) in candidates.into_iter() {
		if backed_candidates.len() == 0 {
			defensive!("Backed candidates for paraid {} is empty.", para_id);
			continue
		}

		let scheduled_cores = scheduled.get_mut(&para_id);

		// ParaIds without scheduled cores are silently filtered out.
		if let Some(scheduled_cores) = scheduled_cores {
			if scheduled_cores.len() == 0 {
				log::debug!(
					target: LOG_TARGET,
					"Paraid: {:?} has no scheduled cores but {} candidates were supplied.",
					para_id,
					backed_candidates.len()
				);

			// Non-elastic scaling case. One core per para.
			} else if scheduled_cores.len() == 1 && !core_index_enabled {
				backed_candidates_with_core.insert(
					para_id,
					vec![(
						// We need the first one here, as we assume candidates of a para are in
						// dependency order.
						backed_candidates.into_iter().next().expect("Length is at least 1"),
						scheduled_cores.pop_first().expect("Length is 1"),
					)],
				);
				continue;

			// Elastic scaling case. We only allow candidates which have the right core
			// indices injected.
			} else if scheduled_cores.len() >= 1 && core_index_enabled {
				// We must preserve the dependency order given in the input.
				let mut temp_backed_candidates = Vec::with_capacity(scheduled_cores.len());

				for candidate in backed_candidates {
					if scheduled_cores.len() == 0 {
						// We've got candidates for all of this para's assigned cores. Move on to
						// the next para.
						log::debug!(
							target: LOG_TARGET,
							"Found enough candidates for paraid: {:?}.",
							candidate.descriptor().para_id
						);
						break;
					}
					let maybe_injected_core_index: Option<CoreIndex> =
						get_injected_core_index::<T>(allowed_relay_parents, &candidate);

					if let Some(core_index) = maybe_injected_core_index {
						if scheduled_cores.remove(&core_index) {
							temp_backed_candidates.push((candidate, core_index));
						} else {
							// if we got a candidate for a core index which is not scheduled, stop
							// the work for this para. the already processed candidate chain in
							// temp_backed_candidates is still fine though.
							log::debug!(
								target: LOG_TARGET,
								"Found a backed candidate {:?} with injected core index {}, which is not scheduled for paraid {:?}.",
								candidate.candidate().hash(),
								core_index.0,
								candidate.descriptor().para_id
							);

							break;
						}
					} else {
						// if we got a candidate which does not contain its core index, stop the
						// work for this para. the already processed candidate chain in
						// temp_backed_candidates is still fine though.

						log::debug!(
							target: LOG_TARGET,
							"Found a backed candidate {:?} with no injected core index, for paraid {:?} which has multiple scheduled cores.",
							candidate.candidate().hash(),
							candidate.descriptor().para_id
						);

						break;
					}
				}

				if !temp_backed_candidates.is_empty() {
					backed_candidates_with_core
						.entry(para_id)
						.or_insert_with(|| vec![])
						.extend(temp_backed_candidates);
				}
			} else {
				log::warn!(
					target: LOG_TARGET,
					"Found a paraid {:?} which has multiple scheduled cores but ElasticScalingMVP feature is not enabled: {:?}",
					para_id,
					scheduled_cores
				);
			}
		} else {
			log::debug!(
				target: LOG_TARGET,
				"Paraid: {:?} has no scheduled cores but {} candidates were supplied.",
				para_id,
				backed_candidates.len()
			);
		}
	}

	backed_candidates_with_core
}

fn get_injected_core_index<T: configuration::Config + scheduler::Config + inclusion::Config>(
	allowed_relay_parents: &AllowedRelayParentsTracker<T::Hash, BlockNumberFor<T>>,
	candidate: &BackedCandidate<T::Hash>,
) -> Option<CoreIndex> {
	// After stripping the 8 bit extensions, the `validator_indices` field length is expected
	// to be equal to backing group size. If these don't match, the `CoreIndex` is badly encoded,
	// or not supported.
	let (validator_indices, maybe_core_idx) = candidate.validator_indices_and_core_index(true);

	let Some(core_idx) = maybe_core_idx else { return None };

	let relay_parent_block_number =
		match allowed_relay_parents.acquire_info(candidate.descriptor().relay_parent, None) {
			Some((_, block_num)) => block_num,
			None => {
				log::debug!(
					target: LOG_TARGET,
					"Relay parent {:?} for candidate {:?} is not in the allowed relay parents.",
					candidate.descriptor().relay_parent,
					candidate.candidate().hash(),
				);
				return None
			},
		};

	// Get the backing group of the candidate backed at `core_idx`.
	let group_idx = match <scheduler::Pallet<T>>::group_assigned_to_core(
		core_idx,
		relay_parent_block_number + One::one(),
	) {
		Some(group_idx) => group_idx,
		None => {
			log::debug!(
				target: LOG_TARGET,
				"Can't get the group index for core idx {:?}.",
				core_idx,
			);
			return None
		},
	};

	let group_validators = match <scheduler::Pallet<T>>::group_validators(group_idx) {
		Some(validators) => validators,
		None => return None,
	};

	if group_validators.len() == validator_indices.len() {
		Some(core_idx)
	} else {
		log::debug!(
			target: LOG_TARGET,
			"Expected validator_indices count different than the real one: {}, {} for candidate {:?}",
			group_validators.len(),
			validator_indices.len(),
			candidate.candidate().hash()
		);

		None
	}
}
