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

use crate::{
	configuration, inclusion, initializer, paras,
	paras::ParaKind,
	paras_inherent,
	scheduler::{self, common::AssignmentProvider, CoreOccupied, ParasEntry},
	session_info, shared,
};
use bitvec::{order::Lsb0 as BitOrderLsb0, vec::BitVec};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use primitives::{
	collator_signature_payload, node_features::FeatureIndex, AvailabilityBitfield, BackedCandidate,
	CandidateCommitments, CandidateDescriptor, CandidateHash, CollatorId, CollatorSignature,
	CommittedCandidateReceipt, CompactStatement, CoreIndex, DisputeStatement, DisputeStatementSet,
	GroupIndex, HeadData, Id as ParaId, IndexedVec, InherentData as ParachainsInherentData,
	InvalidDisputeStatementKind, PersistedValidationData, SessionIndex, SigningContext,
	UncheckedSigned, ValidDisputeStatementKind, ValidationCode, ValidatorId, ValidatorIndex,
	ValidityAttestation,
};
use sp_core::{sr25519, H256};
use sp_runtime::{
	generic::Digest,
	traits::{Header as HeaderT, One, TrailingZeroInput, Zero},
	RuntimeAppPublic,
};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque},
	prelude::Vec,
	vec,
};

fn mock_validation_code() -> ValidationCode {
	ValidationCode(vec![1, 2, 3])
}

/// Grab an account, seeded by a name and index.
///
/// This is directly from frame-benchmarking. Copy/pasted so we can use it when not compiling with
/// "features = runtime-benchmarks".
fn account<AccountId: Decode>(name: &'static str, index: u32, seed: u32) -> AccountId {
	let entropy = (name, index, seed).using_encoded(sp_io::hashing::blake2_256);
	AccountId::decode(&mut TrailingZeroInput::new(&entropy[..]))
		.expect("infinite input; no invalid input; qed")
}

/// Create a 32 byte slice based on the given number.
fn byte32_slice_from(n: u32) -> [u8; 32] {
	let mut slice = [0u8; 32];
	slice[31] = (n % (1 << 8)) as u8;
	slice[30] = ((n >> 8) % (1 << 8)) as u8;
	slice[29] = ((n >> 16) % (1 << 8)) as u8;
	slice[28] = ((n >> 24) % (1 << 8)) as u8;

	slice
}

/// Paras inherent `enter` benchmark scenario builder.
pub(crate) struct BenchBuilder<T: paras_inherent::Config> {
	/// Active validators. Validators should be declared prior to all other setup.
	validators: Option<IndexedVec<ValidatorIndex, ValidatorId>>,
	/// Starting block number; we expect it to get incremented on session setup.
	block_number: BlockNumberFor<T>,
	/// Starting session; we expect it to get incremented on session setup.
	session: SessionIndex,
	/// Session we want the scenario to take place in. We will roll to this session.
	target_session: u32,
	/// Optionally set the max validators per core; otherwise uses the configuration value.
	max_validators_per_core: Option<u32>,
	/// Optionally set the max validators; otherwise uses the configuration value.
	max_validators: Option<u32>,
	/// Optionally set the number of dispute statements for each candidate.
	dispute_statements: BTreeMap<u32, u32>,
	/// Session index of for each dispute. Index of slice corresponds to a core,
	/// which is offset by the number of entries for `backed_and_concluding_paras`. I.E. if
	/// `backed_and_concluding_paras` has 3 entries, the first index of `dispute_sessions`
	/// will correspond to core index 3. There must be one entry for each core with a dispute
	/// statement set.
	dispute_sessions: Vec<u32>,
	/// Map from para id to number of validity votes. Core indices are generated based on
	/// `elastic_paras` configuration. Each para id in `elastic_paras` gets the
	/// specified amount of consecutive cores assigned to it. If a para id is not present
	/// in `elastic_paras` it get assigned to a single core.
	backed_and_concluding_paras: BTreeMap<u32, u32>,
	/// Map from para id (seed) to number of chained candidates.
	elastic_paras: BTreeMap<u32, u8>,
	/// Make every candidate include a code upgrade by setting this to `Some` where the interior
	/// value is the byte length of the new code.
	code_upgrade: Option<u32>,
	/// Specifies whether the claimqueue should be filled.
	fill_claimqueue: bool,
	/// Cores which should not be available when being populated with pending candidates.
	unavailable_cores: Vec<u32>,
	_phantom: sp_std::marker::PhantomData<T>,
}

/// Paras inherent `enter` benchmark scenario.
#[cfg(any(feature = "runtime-benchmarks", test))]
pub(crate) struct Bench<T: paras_inherent::Config> {
	pub(crate) data: ParachainsInherentData<HeaderFor<T>>,
	pub(crate) _session: u32,
	pub(crate) _block_number: BlockNumberFor<T>,
}

#[allow(dead_code)]
impl<T: paras_inherent::Config> BenchBuilder<T> {
	/// Create a new `BenchBuilder` with some opinionated values that should work with the rest
	/// of the functions in this implementation.
	pub(crate) fn new() -> Self {
		BenchBuilder {
			validators: None,
			block_number: Zero::zero(),
			session: SessionIndex::from(0u32),
			target_session: 2u32,
			max_validators_per_core: None,
			max_validators: None,
			dispute_statements: BTreeMap::new(),
			dispute_sessions: Default::default(),
			backed_and_concluding_paras: Default::default(),
			elastic_paras: Default::default(),
			code_upgrade: None,
			fill_claimqueue: true,
			unavailable_cores: vec![],
			_phantom: sp_std::marker::PhantomData::<T>,
		}
	}

	/// Set the session index for each dispute statement set (in other words, set the session the
	/// the dispute statement set's relay chain block is from). Indexes of `dispute_sessions`
	/// correspond to a core, which is offset by the number of entries for
	/// `backed_and_concluding_paras`. I.E. if `backed_and_concluding_paras` cores has 3 entries,
	/// the first index of `dispute_sessions` will correspond to core index 3.
	///
	/// Note that there must be an entry for each core with a dispute statement set.
	pub(crate) fn set_dispute_sessions(mut self, dispute_sessions: impl AsRef<[u32]>) -> Self {
		self.dispute_sessions = dispute_sessions.as_ref().to_vec();
		self
	}

	/// Set the cores which should not be available when being populated with pending candidates.
	pub(crate) fn set_unavailable_cores(mut self, unavailable_cores: Vec<u32>) -> Self {
		self.unavailable_cores = unavailable_cores;
		self
	}

	/// Set a map from para id seed to number of validity votes.
	pub(crate) fn set_backed_and_concluding_paras(
		mut self,
		backed_and_concluding_paras: BTreeMap<u32, u32>,
	) -> Self {
		self.backed_and_concluding_paras = backed_and_concluding_paras;
		self
	}

	/// Set a map from para id seed to number of cores assigned to it.
	pub(crate) fn set_elastic_paras(mut self, elastic_paras: BTreeMap<u32, u8>) -> Self {
		self.elastic_paras = elastic_paras;
		self
	}

	/// Set to include a code upgrade for all backed candidates. The value will be the byte length
	/// of the code.
	pub(crate) fn set_code_upgrade(mut self, code_upgrade: impl Into<Option<u32>>) -> Self {
		self.code_upgrade = code_upgrade.into();
		self
	}

	/// Mock header.
	pub(crate) fn header(block_number: BlockNumberFor<T>) -> HeaderFor<T> {
		HeaderFor::<T>::new(
			block_number,       // `block_number`,
			Default::default(), // `extrinsics_root`,
			Default::default(), // `storage_root`,
			Default::default(), // `parent_hash`,
			Default::default(), // digest,
		)
	}

	/// Number of the relay parent block.
	fn relay_parent_number(&self) -> u32 {
		(self.block_number - One::one())
			.try_into()
			.map_err(|_| ())
			.expect("self.block_number is u32")
	}

	/// Maximum number of validators that may be part of a validator group.
	pub(crate) fn fallback_max_validators() -> u32 {
		configuration::Pallet::<T>::config().max_validators.unwrap_or(200)
	}

	/// Maximum number of validators participating in parachains consensus (a.k.a. active
	/// validators).
	fn max_validators(&self) -> u32 {
		self.max_validators.unwrap_or(Self::fallback_max_validators())
	}

	/// Set the maximum number of active validators.
	#[cfg(not(feature = "runtime-benchmarks"))]
	pub(crate) fn set_max_validators(mut self, n: u32) -> Self {
		self.max_validators = Some(n);
		self
	}

	/// Maximum number of validators per core (a.k.a. max validators per group). This value is used
	/// if none is explicitly set on the builder.
	pub(crate) fn fallback_max_validators_per_core() -> u32 {
		configuration::Pallet::<T>::config()
			.scheduler_params
			.max_validators_per_core
			.unwrap_or(5)
	}

	/// Specify a mapping of core index/ para id to the number of dispute statements for the
	/// corresponding dispute statement set. Note that if the number of disputes is not specified
	/// it fallbacks to having a dispute per every validator. Additionally, an entry is not
	/// guaranteed to have a dispute - it must line up with the cores marked as disputed as defined
	/// in `Self::Build`.
	#[cfg(not(feature = "runtime-benchmarks"))]
	pub(crate) fn set_dispute_statements(mut self, m: BTreeMap<u32, u32>) -> Self {
		self.dispute_statements = m;
		self
	}

	/// Get the maximum number of validators per core.
	fn max_validators_per_core(&self) -> u32 {
		self.max_validators_per_core.unwrap_or(Self::fallback_max_validators_per_core())
	}

	/// Set maximum number of validators per core.
	#[cfg(not(feature = "runtime-benchmarks"))]
	pub(crate) fn set_max_validators_per_core(mut self, n: u32) -> Self {
		self.max_validators_per_core = Some(n);
		self
	}

	/// Get the maximum number of cores we expect from this configuration.
	pub(crate) fn max_cores(&self) -> u32 {
		self.max_validators() / self.max_validators_per_core()
	}

	/// Set whether the claim queue should be filled.
	#[cfg(not(feature = "runtime-benchmarks"))]
	pub(crate) fn set_fill_claimqueue(mut self, f: bool) -> Self {
		self.fill_claimqueue = f;
		self
	}

	/// Get the minimum number of validity votes in order for a backed candidate to be included.
	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn fallback_min_validity_votes() -> u32 {
		(Self::fallback_max_validators() / 2) + 1
	}

	fn mock_head_data() -> HeadData {
		let max_head_size = configuration::Pallet::<T>::config().max_head_data_size;
		HeadData(vec![0xFF; max_head_size as usize])
	}

	fn candidate_descriptor_mock() -> CandidateDescriptor<T::Hash> {
		CandidateDescriptor::<T::Hash> {
			para_id: 0.into(),
			relay_parent: Default::default(),
			collator: CollatorId::from(sr25519::Public::from_raw([42u8; 32])),
			persisted_validation_data_hash: Default::default(),
			pov_hash: Default::default(),
			erasure_root: Default::default(),
			signature: CollatorSignature::from(sr25519::Signature::from_raw([42u8; 64])),
			para_head: Default::default(),
			validation_code_hash: mock_validation_code().hash(),
		}
	}

	/// Create a mock of `CandidatePendingAvailability`.
	fn candidate_availability_mock(
		group_idx: GroupIndex,
		core_idx: CoreIndex,
		candidate_hash: CandidateHash,
		availability_votes: BitVec<u8, BitOrderLsb0>,
		commitments: CandidateCommitments,
	) -> inclusion::CandidatePendingAvailability<T::Hash, BlockNumberFor<T>> {
		inclusion::CandidatePendingAvailability::<T::Hash, BlockNumberFor<T>>::new(
			core_idx,                          // core
			candidate_hash,                    // hash
			Self::candidate_descriptor_mock(), // candidate descriptor
			commitments,                       // commitments
			availability_votes,                // availability votes
			Default::default(),                // backers
			Zero::zero(),                      // relay parent
			One::one(),                        // relay chain block this was backed in
			group_idx,                         // backing group
		)
	}

	/// Add `CandidatePendingAvailability` and `CandidateCommitments` to the relevant storage items.
	///
	/// NOTE: the default `CandidateCommitments` used does not include any data that would lead to
	/// heavy code paths in `enact_candidate`. But enact_candidates does return a weight which will
	/// get taken into account.
	fn add_availability(
		para_id: ParaId,
		core_idx: CoreIndex,
		group_idx: GroupIndex,
		availability_votes: BitVec<u8, BitOrderLsb0>,
		candidate_hash: CandidateHash,
	) {
		let commitments = CandidateCommitments::<u32> {
			upward_messages: Default::default(),
			horizontal_messages: Default::default(),
			new_validation_code: None,
			head_data: Self::mock_head_data(),
			processed_downward_messages: 0,
			hrmp_watermark: 0u32.into(),
		};
		let candidate_availability = Self::candidate_availability_mock(
			group_idx,
			core_idx,
			candidate_hash,
			availability_votes,
			commitments,
		);
		inclusion::PendingAvailability::<T>::mutate(para_id, |maybe_andidates| {
			if let Some(candidates) = maybe_andidates {
				candidates.push_back(candidate_availability);
			} else {
				*maybe_andidates =
					Some([candidate_availability].into_iter().collect::<VecDeque<_>>());
			}
		});
	}

	/// Create an `AvailabilityBitfield` where `concluding` is a map where each key is a core index
	/// that is concluding and `cores` is the total number of cores in the system.
	fn availability_bitvec(concluding_cores: &BTreeSet<u32>, cores: usize) -> AvailabilityBitfield {
		let mut bitfields = bitvec::bitvec![u8, bitvec::order::Lsb0; 0; 0];
		for i in 0..cores {
			if concluding_cores.contains(&(i as u32)) {
				bitfields.push(true);
			} else {
				bitfields.push(false)
			}
		}

		bitfields.into()
	}

	/// Run to block number `to`, calling `initializer` `on_initialize` and `on_finalize` along the
	/// way.
	fn run_to_block(to: u32) {
		let to = to.into();
		while frame_system::Pallet::<T>::block_number() < to {
			let b = frame_system::Pallet::<T>::block_number();
			initializer::Pallet::<T>::on_finalize(b);

			let b = b + One::one();
			frame_system::Pallet::<T>::set_block_number(b);
			initializer::Pallet::<T>::on_initialize(b);
		}
	}

	/// Register `n_paras` count of parachains.
	///
	/// Note that this must be called at least 2 sessions before the target session as there is a
	/// n+2 session delay for the scheduled actions to take effect.
	fn setup_para_ids(n_paras: usize) {
		// make sure parachains exist prior to session change.
		for i in 0..n_paras {
			let para_id = ParaId::from(i as u32);
			let validation_code = mock_validation_code();

			paras::Pallet::<T>::schedule_para_initialize(
				para_id,
				paras::ParaGenesisArgs {
					genesis_head: Self::mock_head_data(),
					validation_code: validation_code.clone(),
					para_kind: ParaKind::Parachain,
				},
			)
			.unwrap();
			paras::Pallet::<T>::add_trusted_validation_code(
				frame_system::Origin::<T>::Root.into(),
				validation_code,
			)
			.unwrap();
		}
	}

	/// Generate validator key pairs and account ids.
	fn generate_validator_pairs(validator_count: u32) -> Vec<(T::AccountId, ValidatorId)> {
		(0..validator_count)
			.map(|i| {
				let public = ValidatorId::generate_pair(None);

				// The account Id is not actually used anywhere, just necessary to fulfill the
				// expected type of the `validators` param of `test_trigger_on_new_session`.
				let account: T::AccountId = account("validator", i, i);
				(account, public)
			})
			.collect()
	}

	fn signing_context(&self) -> SigningContext<T::Hash> {
		SigningContext {
			parent_hash: Self::header(self.block_number).hash(),
			session_index: self.session,
		}
	}

	/// Create a bitvec of `validators` length with all yes votes.
	fn validator_availability_votes_yes(validators: usize) -> BitVec<u8, bitvec::order::Lsb0> {
		// every validator confirms availability.
		bitvec::bitvec![u8, bitvec::order::Lsb0; 1; validators as usize]
	}

	/// Setup session 1 and create `self.validators_map` and `self.validators`.
	fn setup_session(
		mut self,
		target_session: SessionIndex,
		validators: Vec<(T::AccountId, ValidatorId)>,
		// Total cores used in the scenario
		total_cores: usize,
		// Additional cores for elastic parachains
		extra_cores: usize,
	) -> Self {
		let mut block = 1;
		for session in 0..=target_session {
			initializer::Pallet::<T>::test_trigger_on_new_session(
				false,
				session,
				validators.iter().map(|(a, v)| (a, v.clone())),
				None,
			);
			block += 1;
			Self::run_to_block(block);
		}

		let block_number = BlockNumberFor::<T>::from(block);
		let header = Self::header(block_number);

		frame_system::Pallet::<T>::reset_events();
		frame_system::Pallet::<T>::initialize(
			&header.number(),
			&header.hash(),
			&Digest { logs: Vec::new() },
		);

		assert_eq!(<shared::Pallet<T>>::session_index(), target_session);

		// We need to refetch validators since they have been shuffled.
		let validators_shuffled = session_info::Pallet::<T>::session_info(target_session)
			.unwrap()
			.validators
			.clone();

		self.validators = Some(validators_shuffled);
		self.block_number = block_number;
		self.session = target_session;

		assert_eq!(paras::Pallet::<T>::parachains().len(), total_cores - extra_cores);

		self
	}

	/// Create a `UncheckedSigned<AvailabilityBitfield> for each validator where each core in
	/// `concluding_cores` is fully available. Additionally set up storage such that each
	/// `concluding_cores`is pending becoming fully available so the generated bitfields will be
	///  to the cores successfully being freed from the candidates being marked as available.
	fn create_availability_bitfields(
		&self,
		concluding_paras: &BTreeMap<u32, u32>,
		elastic_paras: &BTreeMap<u32, u8>,
		total_cores: usize,
	) -> Vec<UncheckedSigned<AvailabilityBitfield>> {
		let validators =
			self.validators.as_ref().expect("must have some validators prior to calling");

		let mut current_core_idx = 0u32;
		let mut concluding_cores = BTreeSet::new();

		for (seed, _) in concluding_paras.iter() {
			// make sure the candidates that will be concluding are marked as pending availability.
			let para_id = ParaId::from(*seed);

			for _chain_idx in 0..elastic_paras.get(&seed).cloned().unwrap_or(1) {
				let core_idx = CoreIndex::from(current_core_idx);
				let group_idx =
					scheduler::Pallet::<T>::group_assigned_to_core(core_idx, self.block_number)
						.unwrap();

				Self::add_availability(
					para_id,
					core_idx,
					group_idx,
					// No validators have made this candidate available yet.
					bitvec::bitvec![u8, bitvec::order::Lsb0; 0; validators.len()],
					CandidateHash(H256::from(byte32_slice_from(current_core_idx))),
				);
				if !self.unavailable_cores.contains(&current_core_idx) {
					concluding_cores.insert(current_core_idx);
				}
				current_core_idx += 1;
			}
		}

		let availability_bitvec = Self::availability_bitvec(&concluding_cores, total_cores);

		let bitfields: Vec<UncheckedSigned<AvailabilityBitfield>> = validators
			.iter()
			.enumerate()
			.map(|(i, public)| {
				let unchecked_signed = UncheckedSigned::<AvailabilityBitfield>::benchmark_sign(
					public,
					availability_bitvec.clone(),
					&self.signing_context(),
					ValidatorIndex(i as u32),
				);

				unchecked_signed
			})
			.collect();

		bitfields
	}

	/// Create backed candidates for `cores_with_backed_candidates`. You need these cores to be
	/// scheduled _within_ paras inherent, which requires marking the available bitfields as fully
	/// available.
	/// - `cores_with_backed_candidates` Mapping of `para_id` seed to number of
	/// validity votes.
	fn create_backed_candidates(
		&self,
		paras_with_backed_candidates: &BTreeMap<u32, u32>,
		elastic_paras: &BTreeMap<u32, u8>,
		includes_code_upgrade: Option<u32>,
	) -> Vec<BackedCandidate<T::Hash>> {
		let validators =
			self.validators.as_ref().expect("must have some validators prior to calling");
		let config = configuration::Pallet::<T>::config();

		let mut current_core_idx = 0u32;
		paras_with_backed_candidates
			.iter()
			.flat_map(|(seed, num_votes)| {
				assert!(*num_votes <= validators.len() as u32);

				let para_id = ParaId::from(*seed);
				let mut prev_head = None;
				// How many chained candidates we want to build ?
				(0..elastic_paras.get(&seed).cloned().unwrap_or(1))
					.map(|chain_idx| {
						let core_idx = CoreIndex::from(current_core_idx);
						// Advance core index.
						current_core_idx += 1;
						let group_idx = scheduler::Pallet::<T>::group_assigned_to_core(
							core_idx,
							self.block_number,
						)
						.unwrap();

						// This generates a pair and adds it to the keystore, returning just the
						// public.
						let collator_public = CollatorId::generate_pair(None);
						let header = Self::header(self.block_number);
						let relay_parent = header.hash();

						// Set the head data so it can be used while validating the signatures on
						// the candidate receipt.
						let mut head_data = Self::mock_head_data();

						if chain_idx == 0 {
							// Only first parahead of the chain needs to be set in storage.
							paras::Pallet::<T>::heads_insert(&para_id, head_data.clone());
						} else {
							// Make each candidate head data unique to avoid cycles.
							head_data.0[0] = chain_idx;
						}

						let persisted_validation_data_hash = PersistedValidationData::<H256> {
							// To form a chain we set parent head to previous block if any, or
							// default to what is in storage already setup.
							parent_head: prev_head.take().unwrap_or(head_data.clone()),
							relay_parent_number: self.relay_parent_number(),
							relay_parent_storage_root: Default::default(),
							max_pov_size: config.max_pov_size,
						}
						.hash();

						prev_head = Some(head_data.clone());

						let pov_hash = Default::default();
						let validation_code_hash = mock_validation_code().hash();
						let payload = collator_signature_payload(
							&relay_parent,
							&para_id,
							&persisted_validation_data_hash,
							&pov_hash,
							&validation_code_hash,
						);
						let signature = collator_public.sign(&payload).unwrap();

						let mut past_code_meta =
							paras::ParaPastCodeMeta::<BlockNumberFor<T>>::default();
						past_code_meta.note_replacement(0u32.into(), 0u32.into());

						let group_validators =
							scheduler::Pallet::<T>::group_validators(group_idx).unwrap();

						let candidate = CommittedCandidateReceipt::<T::Hash> {
							descriptor: CandidateDescriptor::<T::Hash> {
								para_id,
								relay_parent,
								collator: collator_public,
								persisted_validation_data_hash,
								pov_hash,
								erasure_root: Default::default(),
								signature,
								para_head: head_data.hash(),
								validation_code_hash,
							},
							commitments: CandidateCommitments::<u32> {
								upward_messages: Default::default(),
								horizontal_messages: Default::default(),
								new_validation_code: includes_code_upgrade
									.map(|v| ValidationCode(vec![42u8; v as usize])),
								head_data,
								processed_downward_messages: 0,
								hrmp_watermark: self.relay_parent_number(),
							},
						};

						let candidate_hash = candidate.hash();

						let validity_votes: Vec<_> = group_validators
							.iter()
							.take(*num_votes as usize)
							.map(|val_idx| {
								let public = validators.get(*val_idx).unwrap();
								let sig = UncheckedSigned::<CompactStatement>::benchmark_sign(
									public,
									CompactStatement::Valid(candidate_hash),
									&self.signing_context(),
									*val_idx,
								)
								.benchmark_signature();

								ValidityAttestation::Explicit(sig.clone())
							})
							.collect();

						// Check if the elastic scaling bit is set, if so we need to supply the core
						// index in the generated candidate.
						let core_idx = configuration::Pallet::<T>::config()
							.node_features
							.get(FeatureIndex::ElasticScalingMVP as usize)
							.map(|_the_bit| core_idx);

						BackedCandidate::<T::Hash>::new(
							candidate,
							validity_votes,
							bitvec::bitvec![u8, bitvec::order::Lsb0; 1; group_validators.len()],
							core_idx,
						)
					})
					.collect::<Vec<_>>()
			})
			.collect()
	}

	/// Fill cores `start..last` with dispute statement sets. The statement sets will have 3/4th of
	/// votes be valid, and 1/4th of votes be invalid.
	fn create_disputes(
		&self,
		start: u32,
		last: u32,
		dispute_sessions: impl AsRef<[u32]>,
	) -> Vec<DisputeStatementSet> {
		let validators =
			self.validators.as_ref().expect("must have some validators prior to calling");

		let dispute_sessions = dispute_sessions.as_ref();
		let mut current_core_idx = start;

		(start..last)
			.map(|seed| {
				let dispute_session_idx = (seed - start) as usize;
				let session = dispute_sessions
					.get(dispute_session_idx)
					.cloned()
					.unwrap_or(self.target_session);

				let para_id = ParaId::from(seed);
				let core_idx = CoreIndex::from(current_core_idx);
				current_core_idx +=1;

				let group_idx =
					scheduler::Pallet::<T>::group_assigned_to_core(core_idx, self.block_number)
						.unwrap();

				let candidate_hash = CandidateHash(H256::from(byte32_slice_from(seed)));
				let relay_parent = H256::from(byte32_slice_from(seed));

				Self::add_availability(
					para_id,
					core_idx,
					group_idx,
					Self::validator_availability_votes_yes(validators.len()),
					candidate_hash,
				);

				let statements_len =
					self.dispute_statements.get(&seed).cloned().unwrap_or(validators.len() as u32);
				let statements = (0..statements_len)
					.map(|validator_index| {
						let validator_public = &validators.get(ValidatorIndex::from(validator_index)).expect("Test case is not borked. `ValidatorIndex` out of bounds of `ValidatorId`s.");

						// We need dispute statements on each side. And we don't want a revert log
						// so we make sure that we have a super majority with valid statements.
						let dispute_statement = if validator_index % 4 == 0 {
							DisputeStatement::Invalid(InvalidDisputeStatementKind::Explicit)
						} else if validator_index < 3 {
							// Set two votes as backing for the dispute set to be accepted
							DisputeStatement::Valid(
								ValidDisputeStatementKind::BackingValid(relay_parent)
							)
						} else {
							DisputeStatement::Valid(ValidDisputeStatementKind::Explicit)
						};
						let data = dispute_statement.payload_data(candidate_hash, session).unwrap();
						let statement_sig = validator_public.sign(&data).unwrap();

						(dispute_statement, ValidatorIndex(validator_index), statement_sig)
					})
					.collect();

				DisputeStatementSet { candidate_hash, session, statements }
			})
			.collect()
	}

	/// Build a scenario for testing or benchmarks.
	///
	/// Note that this API only allows building scenarios where the `backed_and_concluding_paras`
	/// are mutually exclusive with the cores for disputes. So
	/// `backed_and_concluding_paras.len() + dispute_sessions.len()` must be less than the max
	/// number of cores.
	pub(crate) fn build(self) -> Bench<T> {
		// Make sure relevant storage is cleared. This is just to get the asserts to work when
		// running tests because it seems the storage is not cleared in between.
		#[allow(deprecated)]
		inclusion::PendingAvailability::<T>::remove_all(None);

		// We don't allow a core to have both disputes and be marked fully available at this block.
		let max_cores = self.max_cores() as usize;

		let extra_cores = self
			.elastic_paras
			.values()
			.map(|count| *count as usize)
			.sum::<usize>()
			.saturating_sub(self.elastic_paras.len() as usize);

		let used_cores =
			self.dispute_sessions.len() + self.backed_and_concluding_paras.len() + extra_cores;

		assert!(used_cores <= max_cores);
		let fill_claimqueue = self.fill_claimqueue;

		// NOTE: there is an n+2 session delay for these actions to take effect.
		// We are currently in Session 0, so these changes will take effect in Session 2.
		Self::setup_para_ids(used_cores - extra_cores);
		configuration::ActiveConfig::<T>::mutate(|c| {
			c.scheduler_params.num_cores = used_cores as u32;
		});

		let validator_ids = Self::generate_validator_pairs(self.max_validators());
		let target_session = SessionIndex::from(self.target_session);
		let builder = self.setup_session(target_session, validator_ids, used_cores, extra_cores);

		let bitfields = builder.create_availability_bitfields(
			&builder.backed_and_concluding_paras,
			&builder.elastic_paras,
			used_cores,
		);
		let backed_candidates = builder.create_backed_candidates(
			&builder.backed_and_concluding_paras,
			&builder.elastic_paras,
			builder.code_upgrade,
		);

		let disputes = builder.create_disputes(
			builder.backed_and_concluding_paras.len() as u32,
			(used_cores - extra_cores) as u32,
			builder.dispute_sessions.as_slice(),
		);
		let mut disputed_cores = (builder.backed_and_concluding_paras.len() as u32..
			((used_cores - extra_cores) as u32))
			.into_iter()
			.map(|idx| (idx, 0))
			.collect::<BTreeMap<_, _>>();

		let mut all_cores = builder.backed_and_concluding_paras.clone();
		all_cores.append(&mut disputed_cores);

		assert_eq!(inclusion::PendingAvailability::<T>::iter().count(), used_cores - extra_cores);

		// Mark all the used cores as occupied. We expect that there are
		// `backed_and_concluding_paras` that are pending availability and that there are
		// `used_cores - backed_and_concluding_paras ` which are about to be disputed.
		let now = <frame_system::Pallet<T>>::block_number() + One::one();

		let mut core_idx = 0u32;
		let elastic_paras = &builder.elastic_paras;
		// Assign potentially multiple cores to same parachains,
		let cores = all_cores
			.iter()
			.flat_map(|(para_id, _)| {
				(0..elastic_paras.get(&para_id).cloned().unwrap_or(1))
					.map(|_para_local_core_idx| {
						let ttl = configuration::Pallet::<T>::config().scheduler_params.ttl;
						// Load an assignment into provider so that one is present to pop
						let assignment =
							<T as scheduler::Config>::AssignmentProvider::get_mock_assignment(
								CoreIndex(core_idx),
								ParaId::from(*para_id),
							);
						core_idx += 1;
						CoreOccupied::Paras(ParasEntry::new(assignment, now + ttl))
					})
					.collect::<Vec<CoreOccupied<_>>>()
			})
			.collect::<Vec<CoreOccupied<_>>>();

		scheduler::AvailabilityCores::<T>::set(cores);

		core_idx = 0u32;
		if fill_claimqueue {
			let cores = all_cores
				.keys()
				.flat_map(|para_id| {
					(0..elastic_paras.get(&para_id).cloned().unwrap_or(1))
						.filter_map(|_para_local_core_idx| {
							let ttl = configuration::Pallet::<T>::config().scheduler_params.ttl;
							// Load an assignment into provider so that one is present to pop
							let assignment =
								<T as scheduler::Config>::AssignmentProvider::get_mock_assignment(
									CoreIndex(core_idx),
									ParaId::from(*para_id),
								);

							let entry = (
								CoreIndex(core_idx),
								[ParasEntry::new(assignment, now + ttl)].into(),
							);
							let res = if builder.unavailable_cores.contains(&core_idx) {
								None
							} else {
								Some(entry)
							};
							core_idx += 1;
							res
						})
						.collect::<Vec<(CoreIndex, VecDeque<ParasEntry<_>>)>>()
				})
				.collect::<BTreeMap<CoreIndex, VecDeque<ParasEntry<_>>>>();

			scheduler::ClaimQueue::<T>::set(cores);
		}

		Bench::<T> {
			data: ParachainsInherentData {
				bitfields,
				backed_candidates,
				disputes,
				parent_header: Self::header(builder.block_number),
			},
			_session: target_session,
			_block_number: builder.block_number,
		}
	}
}
