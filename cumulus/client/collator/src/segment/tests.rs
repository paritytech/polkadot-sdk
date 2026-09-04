// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use super::*;

use crate::metrics::Metrics;
use assert_matches::assert_matches;
use async_trait::async_trait;
use codec::Encode;
use cumulus_relay_chain_interface::{
	ChildInfo, CommittedCandidateReceipt, CoreState, InboundDownwardMessage, InboundHrmpMessage,
	OccupiedCoreAssumption, OverseerHandle, PHash, PHeader, ParaId as RelayParaId,
	PersistedValidationData as RCPersistedValidationData, RelayChainError, RelayChainInterface,
	RelayChainResult, SessionIndex as RCSessionIndex, StorageValue, ValidationCodeHash as RCVch,
};
use polkadot_node_primitives::{BlockData, Collation, MaybeCompressedPoV, PoV, SegmentCollation};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage, Segment};
use polkadot_node_subsystem_types::collation::{SchedulingContext, SegmentToDistribute};
use polkadot_node_subsystem_util::metered::MeteredReceiver;
use polkadot_overseer::{Event, Handle};
use polkadot_primitives::{
	transpose_claim_queue, vstaging::RelayParentInfo, BlockId, BlockNumber, CandidateEvent,
	ClaimQueueOffset, CoreIndex, CoreSelector, Hash, HeadData, Id as ParaId, NodeFeatures,
	SessionIndex, UMPSignal, ValidationCodeHash, ValidatorId, UMP_SEPARATOR,
};
use prometheus_endpoint::Registry;
use sc_client_api::StorageProof;
use sp_version::RuntimeVersion;
use std::{
	collections::{BTreeMap, VecDeque},
	pin::Pin,
	sync::{Arc, Mutex},
};

const PARA_ID: ParaId = ParaId::new(5);

fn pvd() -> RCPersistedValidationData {
	RCPersistedValidationData {
		parent_head: HeadData(vec![1, 2, 3]),
		relay_parent_number: 10,
		relay_parent_storage_root: Hash::repeat_byte(1),
		max_pov_size: 1024,
	}
}

fn validation_code_hash() -> ValidationCodeHash {
	Hash::repeat_byte(42).into()
}

fn collation_with_signals(signals: &[UMPSignal]) -> Collation {
	let mut c = Collation {
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		new_validation_code: None,
		head_data: HeadData(vec![1, 2, 3]),
		proof_of_validity: MaybeCompressedPoV::Raw(PoV { block_data: BlockData(vec![]) }),
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	};
	if !signals.is_empty() {
		c.upward_messages.force_push(UMP_SEPARATOR);
		for sig in signals {
			c.upward_messages.force_push(sig.encode());
		}
	}
	c
}

fn segment_collation(collation: Collation, relay_parent: Hash) -> SegmentCollation {
	SegmentCollation {
		collation,
		relay_parent,
		validation_data: pvd(),
		validation_code_hash: validation_code_hash(),
		session_index: 1,
	}
}

/// Mock relay client that records which block hash was passed to each method.
struct MockRelayClient {
	/// Hashes passed to `validators()`.
	validators_calls: Arc<Mutex<Vec<Hash>>>,
	/// Hashes passed to `claim_queue()`.
	claim_queue_calls: Arc<Mutex<Vec<Hash>>>,
	/// The claim queue to return, or `None` to return an error.
	claim_queue_result: Option<BTreeMap<CoreIndex, VecDeque<ParaId>>>,
	/// Number of validators to return.
	n_validators: usize,
}

impl MockRelayClient {
	fn new(
		claim_queue_result: Option<BTreeMap<CoreIndex, VecDeque<ParaId>>>,
		n_validators: usize,
	) -> (Self, Arc<Mutex<Vec<Hash>>>, Arc<Mutex<Vec<Hash>>>) {
		let validators_calls = Arc::new(Mutex::new(Vec::new()));
		let claim_queue_calls = Arc::new(Mutex::new(Vec::new()));
		let client = Self {
			validators_calls: validators_calls.clone(),
			claim_queue_calls: claim_queue_calls.clone(),
			claim_queue_result,
			n_validators,
		};
		(client, validators_calls, claim_queue_calls)
	}
}

#[async_trait]
impl RelayChainInterface for MockRelayClient {
	async fn validators(&self, block_id: PHash) -> RelayChainResult<Vec<ValidatorId>> {
		self.validators_calls.lock().unwrap().push(block_id);
		Ok((0..self.n_validators)
			.map(|_| polkadot_primitives_test_helpers::dummy_validator())
			.collect())
	}

	async fn claim_queue(
		&self,
		relay_parent: PHash,
	) -> RelayChainResult<BTreeMap<CoreIndex, VecDeque<RelayParaId>>> {
		self.claim_queue_calls.lock().unwrap().push(relay_parent);
		match &self.claim_queue_result {
			Some(cq) => Ok(cq.clone()),
			None => Err(RelayChainError::GenericError("test error".into())),
		}
	}

	async fn session_index_for_child(&self, _block_id: PHash) -> RelayChainResult<RCSessionIndex> {
		// `SegmentDistributor` takes the session from `SchedulingContext`; it never asks the
		// runtime, so a value here would imply a read that does not happen.
		unimplemented!("Not needed for test")
	}

	async fn best_block_hash(&self) -> RelayChainResult<PHash> {
		unimplemented!("Not needed for test")
	}

	async fn finalized_block_hash(&self) -> RelayChainResult<PHash> {
		unimplemented!("Not needed for test")
	}

	async fn header(&self, _: BlockId) -> RelayChainResult<Option<PHeader>> {
		unimplemented!("Not needed for test")
	}

	async fn call_runtime_api(
		&self,
		_: &'static str,
		_: PHash,
		_: &[u8],
	) -> RelayChainResult<Vec<u8>> {
		unimplemented!("Not needed for test")
	}

	async fn retrieve_dmq_contents(
		&self,
		_: RelayParaId,
		_: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		unimplemented!("Not needed for test")
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		_: RelayParaId,
		_: PHash,
	) -> RelayChainResult<BTreeMap<RelayParaId, Vec<InboundHrmpMessage>>> {
		unimplemented!("Not needed for test")
	}

	async fn persisted_validation_data(
		&self,
		_: PHash,
		_: RelayParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<RCPersistedValidationData>> {
		unimplemented!("Not needed for test")
	}

	#[allow(deprecated)]
	async fn candidate_pending_availability(
		&self,
		_: PHash,
		_: RelayParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		unimplemented!("Not needed for test")
	}

	async fn candidates_pending_availability(
		&self,
		_: PHash,
		_: RelayParaId,
	) -> RelayChainResult<Vec<CommittedCandidateReceipt>> {
		unimplemented!("Not needed for test")
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn futures::Stream<Item = PHeader> + Send>>> {
		unimplemented!("Not needed for test")
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn futures::Stream<Item = PHeader> + Send>>> {
		unimplemented!("Not needed for test")
	}

	async fn wait_for_block(&self, _: PHash) -> RelayChainResult<()> {
		unimplemented!("Not needed for test")
	}

	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn futures::Stream<Item = PHeader> + Send>>> {
		unimplemented!("Not needed for test")
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		unimplemented!("Not needed for test")
	}

	fn overseer_handle(&self) -> RelayChainResult<OverseerHandle> {
		unimplemented!("Not needed for test")
	}

	async fn prove_read(&self, _: PHash, _: &Vec<Vec<u8>>) -> RelayChainResult<StorageProof> {
		unimplemented!("Not needed for test")
	}

	async fn prove_child_read(
		&self,
		_: PHash,
		_: &ChildInfo,
		_: &[Vec<u8>],
	) -> RelayChainResult<StorageProof> {
		unimplemented!("Not needed for test")
	}

	async fn validation_code_hash(
		&self,
		_: PHash,
		_: RelayParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<RCVch>> {
		unimplemented!("Not needed for test")
	}

	async fn version(&self, _: PHash) -> RelayChainResult<RuntimeVersion> {
		unimplemented!("Not needed for test")
	}

	async fn availability_cores(
		&self,
		_: PHash,
	) -> RelayChainResult<Vec<CoreState<PHash, BlockNumber>>> {
		unimplemented!("Not needed for test")
	}

	async fn scheduling_lookahead(&self, _: PHash) -> RelayChainResult<u32> {
		unimplemented!("Not needed for test")
	}

	async fn candidate_events(&self, _: PHash) -> RelayChainResult<Vec<CandidateEvent>> {
		unimplemented!("Not needed for test")
	}

	async fn max_relay_parent_session_age(&self, _: PHash) -> RelayChainResult<u32> {
		unimplemented!("Not needed for test")
	}

	async fn node_features(&self, _: PHash) -> RelayChainResult<NodeFeatures> {
		unimplemented!("Not needed for test")
	}

	async fn get_storage_by_key(
		&self,
		_: PHash,
		_: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		unimplemented!("Not needed for test")
	}

	async fn ancestor_relay_parent_info(
		&self,
		_: PHash,
		_: SessionIndex,
		_: PHash,
	) -> RelayChainResult<Option<RelayParentInfo<PHash, BlockNumber>>> {
		unimplemented!("Not needed for test")
	}
}

/// Spawn a task that drains the overseer channel and collects `CollatorProtocol` messages.
fn make_overseer_handle() -> (OverseerHandle, MeteredReceiver<Event>) {
	let (tx, rx) = polkadot_node_subsystem_util::metered::channel(100);
	(Handle::new(tx), rx)
}

/// The collator protocol messages sent so far. `Handle::send_msg` only returns once the message
/// is queued, so draining synchronously after a `distribute` call needs no synchronisation.
fn drain(rx: &mut MeteredReceiver<Event>) -> Vec<CollatorProtocolMessage> {
	let mut messages = Vec::new();
	while let Ok(Some(event)) = rx.try_next() {
		if let Event::MsgToSubsystem { msg: AllMessages::CollatorProtocol(msg), .. } = event {
			messages.push(msg);
		}
	}
	messages
}

fn claim_queue_for_core(core: u32) -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
	[(CoreIndex(core), VecDeque::from([PARA_ID]))].into()
}

// ---- Tests ----

/// V3 segments must query `claim_queue` and `validators` against the scheduling parent,
/// not the collation's individual relay parent.
#[tokio::test]
async fn v3_queries_scheduling_parent() {
	let relay_parent = Hash::repeat_byte(0xAA);
	let scheduling_parent = Hash::repeat_byte(0xBB);

	let (client, validators_calls, claim_queue_calls) =
		MockRelayClient::new(Some(claim_queue_for_core(0)), 4);
	let (overseer_handle, mut rx) = make_overseer_handle();

	let mut distributor =
		SegmentDistributor::new(client, overseer_handle, PARA_ID, Default::default());

	distributor
		.distribute(
			SegmentToDistribute {
				core_index: CoreIndex(0),
				scheduling: SchedulingContext::V3 { scheduling_parent, scheduling_session: 7 },
				collations: vec![segment_collation(
					collation_with_signals(&[UMPSignal::SelectCore(
						CoreSelector(0),
						ClaimQueueOffset(0),
					)]),
					relay_parent,
				)],
			},
			None,
		)
		.await;

	// Both calls must use the scheduling_parent, not relay_parent.
	assert_eq!(validators_calls.lock().unwrap().as_slice(), [scheduling_parent]);
	assert_eq!(claim_queue_calls.lock().unwrap().as_slice(), [scheduling_parent]);

	let msgs = drain(&mut rx);
	assert_matches!(
		&msgs[..],
		[CollatorProtocolMessage::DistributeSegment {
			segment: Segment::V3 { scheduling_parent: sp, scheduling_session, candidates },
			..
		}] => {
			assert_eq!(*sp, scheduling_parent);
			assert_eq!(*scheduling_session, 7u32);
			assert_eq!(candidates.len(), 1);
			assert_eq!(candidates[0].relay_parent, relay_parent);
		}
	);
}

/// V2 segments (scheduling_parent == relay_parent) produce a `Segment::V2`.
#[tokio::test]
async fn v2_shape() {
	let relay_parent = Hash::repeat_byte(0x01);

	let (client, _validators_calls, _claim_queue_calls) =
		MockRelayClient::new(Some(claim_queue_for_core(0)), 4);
	let (overseer_handle, mut rx) = make_overseer_handle();

	let mut distributor =
		SegmentDistributor::new(client, overseer_handle, PARA_ID, Default::default());

	distributor
		.distribute(
			SegmentToDistribute {
				core_index: CoreIndex(0),
				scheduling: SchedulingContext::V2 { relay_parent, session: 1 },
				collations: vec![segment_collation(
					collation_with_signals(&[UMPSignal::SelectCore(
						CoreSelector(0),
						ClaimQueueOffset(0),
					)]),
					relay_parent,
				)],
			},
			None,
		)
		.await;

	let msgs = drain(&mut rx);
	assert_matches!(
		&msgs[..],
		[CollatorProtocolMessage::DistributeSegment { segment: Segment::V2(_), .. }] => {}
	);
}

/// Two `distribute` calls with the same session index must query `validators()` only once.
#[tokio::test]
async fn validator_count_cached_within_session() {
	let relay_parent = Hash::repeat_byte(0x02);

	let (client, validators_calls, _claim_queue_calls) =
		MockRelayClient::new(Some(claim_queue_for_core(0)), 4);
	let (overseer_handle, _rx) = make_overseer_handle();

	let mut distributor =
		SegmentDistributor::new(client, overseer_handle, PARA_ID, Default::default());

	let make_segment = || SegmentToDistribute {
		core_index: CoreIndex(0),
		scheduling: SchedulingContext::V3 {
			scheduling_parent: relay_parent,
			scheduling_session: 5,
		},
		collations: vec![segment_collation(
			collation_with_signals(&[UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0))]),
			relay_parent,
		)],
	};

	distributor.distribute(make_segment(), None).await;
	distributor.distribute(make_segment(), None).await;

	// Only one validators() call for both distributes since the session didn't change.
	assert_eq!(validators_calls.lock().unwrap().len(), 1);
}

/// A `claim_queue` error must be silent and non-fatal: no message reaches the collator protocol.
#[tokio::test]
async fn claim_queue_error_is_non_fatal() {
	let relay_parent = Hash::repeat_byte(0x03);

	// Pass `None` so `claim_queue()` returns an error.
	let (client, _validators_calls, _claim_queue_calls) = MockRelayClient::new(None, 4);
	let (overseer_handle, mut rx) = make_overseer_handle();

	let mut distributor =
		SegmentDistributor::new(client, overseer_handle, PARA_ID, Default::default());

	distributor
		.distribute(
			SegmentToDistribute {
				core_index: CoreIndex(0),
				scheduling: SchedulingContext::V3 {
					scheduling_parent: relay_parent,
					scheduling_session: 1,
				},
				collations: vec![segment_collation(
					collation_with_signals(&[UMPSignal::SelectCore(
						CoreSelector(0),
						ClaimQueueOffset(0),
					)]),
					relay_parent,
				)],
			},
			None,
		)
		.await;

	// No message should have been sent.
	assert!(drain(&mut rx).is_empty());
}

/// Read the current value of `polkadot_parachain_collations_generated_total` from a registry.
fn counter_value(registry: &Registry) -> f64 {
	registry
		.gather()
		.into_iter()
		.find(|mf| mf.get_name() == "polkadot_parachain_collations_generated_total")
		.and_then(|mf| mf.get_metric().first().cloned())
		.map(|m| m.get_counter().get_value())
		.unwrap_or(0.0)
}

/// The counter must advance by `candidates.len()` for a V3 segment carrying multiple candidates.
#[tokio::test]
async fn counter_increments_per_candidate_v3() {
	let relay_parent = Hash::repeat_byte(0x10);
	let scheduling_parent = Hash::repeat_byte(0x11);

	// Claim queue must have slots for two candidates on core 0.
	let claim_queue = [(CoreIndex(0), VecDeque::from([PARA_ID, PARA_ID]))].into();
	let (client, _validators_calls, _claim_queue_calls) =
		MockRelayClient::new(Some(claim_queue), 4);
	let (overseer_handle, _rx) = make_overseer_handle();

	let registry = Registry::new();
	let metrics = Metrics::register(Some(&registry)).expect("metrics registered; qed");
	let mut distributor = SegmentDistributor::new(client, overseer_handle, PARA_ID, metrics);

	assert_eq!(counter_value(&registry), 0.0);

	distributor
		.distribute(
			SegmentToDistribute {
				core_index: CoreIndex(0),
				scheduling: SchedulingContext::V3 { scheduling_parent, scheduling_session: 1 },
				collations: vec![
					segment_collation(
						collation_with_signals(&[UMPSignal::SelectCore(
							CoreSelector(0),
							ClaimQueueOffset(0),
						)]),
						relay_parent,
					),
					segment_collation(
						collation_with_signals(&[UMPSignal::SelectCore(
							CoreSelector(0),
							ClaimQueueOffset(1),
						)]),
						relay_parent,
					),
				],
			},
			None,
		)
		.await;

	// Two candidates in the segment → counter must be 2.
	assert_eq!(counter_value(&registry), 2.0);
}

/// A build failure (invalid claim queue) must not increment the counter.
#[tokio::test]
async fn counter_not_incremented_on_build_failure() {
	let relay_parent = Hash::repeat_byte(0x20);

	// Pass `None` so `claim_queue()` returns an error, causing `build_segment` to never run.
	let (client, _validators_calls, _claim_queue_calls) = MockRelayClient::new(None, 4);
	let (overseer_handle, _rx) = make_overseer_handle();

	let registry = Registry::new();
	let metrics = Metrics::register(Some(&registry)).expect("metrics registered; qed");
	let mut distributor = SegmentDistributor::new(client, overseer_handle, PARA_ID, metrics);

	distributor
		.distribute(
			SegmentToDistribute {
				core_index: CoreIndex(0),
				scheduling: SchedulingContext::V3 {
					scheduling_parent: relay_parent,
					scheduling_session: 1,
				},
				collations: vec![segment_collation(
					collation_with_signals(&[UMPSignal::SelectCore(
						CoreSelector(0),
						ClaimQueueOffset(0),
					)]),
					relay_parent,
				)],
			},
			None,
		)
		.await;

	// The build failed; counter must remain at zero.
	assert_eq!(counter_value(&registry), 0.0);
}

/// A caller that already holds the claim queue at the anchor must suppress the runtime call.
/// Re-fetching here costs an uncached round trip per core in the window between authoring and
/// advertisement, so this asserts the fetch does not happen.
#[tokio::test]
async fn supplied_claim_queue_suppresses_the_fetch() {
	let relay_parent = Hash::repeat_byte(0x07);
	let (client, _validators_calls, claim_queue_calls) =
		MockRelayClient::new(Some(claim_queue_for_core(0)), 4);
	let (overseer_handle, mut rx) = make_overseer_handle();
	let mut distributor =
		SegmentDistributor::new(client, overseer_handle, PARA_ID, Default::default());

	distributor
		.distribute(
			SegmentToDistribute {
				core_index: CoreIndex(0),
				scheduling: SchedulingContext::V2 { relay_parent, session: 1 },
				collations: vec![segment_collation(
					collation_with_signals(&[UMPSignal::SelectCore(
						CoreSelector(0),
						ClaimQueueOffset(0),
					)]),
					relay_parent,
				)],
			},
			Some(transpose_claim_queue(claim_queue_for_core(0))),
		)
		.await;

	// No claim-queue runtime call, and the segment still went out.
	assert!(claim_queue_calls.lock().unwrap().is_empty());
	assert_eq!(drain(&mut rx).len(), 1);
}
