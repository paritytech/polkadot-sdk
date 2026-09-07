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

use super::{
	block_builder_task::{determine_cores, offset_relay_parent_find_descendants},
	collation_task::handle_collation_message,
	relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
	CollatorMessage,
};
use async_trait::async_trait;
use codec::Encode;
use cumulus_primitives_core::CoreSelector;
use cumulus_relay_chain_interface::*;
use futures::Stream;
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	vstaging::RelayParentInfo, CandidateEvent, CommittedCandidateReceiptV2, CoreIndex,
	Hash as RelayHash, Header as RelayHeader, Id as ParaId, NodeFeatures,
};
use rstest::rstest;
use sc_consensus_babe::{
	AuthorityId, ConsensusLog as BabeConsensusLog, NextEpochDescriptor, BABE_ENGINE_ID,
};
use sp_core::sr25519;
use sp_runtime::{generic::BlockId, traits::Header};
use sp_version::RuntimeVersion;
use std::{
	collections::{BTreeMap, HashMap, VecDeque},
	pin::Pin,
	sync::{Arc, Mutex},
};

const N_VALIDATORS: usize = 4;
const SESSION_INDEX: SessionIndex = 3;

fn header_numbers(headers: &Vec<RelayHeader>) -> Vec<BlockNumber> {
	headers.iter().map(|header| header.number).collect()
}

#[tokio::test]
async fn offset_test_various_correct_offsets() {
	let (headers, best_header) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Offset 0
	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 0, 0).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(data.descendants_len(), 0);
	assert_eq!(*data.relay_parent().number(), 100);
	assert!(data.into_inherent_descendant_list().is_empty());

	// Offset 5
	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 5, 0).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(data.descendants_len(), 5);
	assert_eq!(*data.relay_parent().number(), 95);
	let descendant_list = data.into_inherent_descendant_list();
	assert_eq!(header_numbers(&descendant_list), (95..=100).collect::<Vec<_>>());

	// Offset 99
	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 99, 0).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(data.descendants_len(), 99);
	assert_eq!(*data.relay_parent().number(), 1);
	let descendant_list = data.into_inherent_descendant_list();
	assert_eq!(header_numbers(&descendant_list), (1..=100).collect::<Vec<_>>());
}

#[tokio::test]
async fn offset_test_too_long() {
	let (headers, best_header) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Offset 100: the relay header would be the genesis block => invalid
	let result =
		offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 100, 0).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());

	// Offset 200: the offset is higher than the chain length
	let result =
		offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 200, 0).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());
}

#[derive(PartialEq)]
enum HasEpochChange {
	Yes,
	No,
}

// When the session change is at the RC tip, there is actually no session change
#[tokio::test]
async fn offset_with_session_change_at_rc_tip() {
	let flags = &[
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::Yes,
	];
	let (headers, best_header) = build_headers_with_epoch_flags(flags);
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 0, 0).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 5);
	assert!(data.descendants.is_empty());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 1, 0).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 1, 1).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 4);
	assert_eq!(header_numbers(&data.descendants), vec![5]);

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 2, 0).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header, 2, 1).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 3);
	assert_eq!(header_numbers(&data.descendants), vec![4, 5]);
}

#[tokio::test]
async fn offset_with_1_session_change() {
	let flags = &[
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::Yes,
		HasEpochChange::No,
	];
	let (headers, best_header) = build_headers_with_epoch_flags(flags);
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 0, 0).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 5);
	assert!(data.descendants.is_empty());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 1, 0).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 4);
	assert_eq!(header_numbers(&data.descendants), vec![5]);

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 2, 0).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 2, 1).await;
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 3);
	assert_eq!(header_numbers(&data.descendants), vec![4, 5]);

	let result = offset_relay_parent_find_descendants(&mut cache, best_header, 3, 1).await;
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 2);
	assert_eq!(header_numbers(&data.descendants), vec![3, 4, 5]);
}

#[tokio::test]
async fn offset_with_2_session_changes() {
	let flags = &[
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::No,
		HasEpochChange::Yes,
		HasEpochChange::No,
		HasEpochChange::Yes,
		HasEpochChange::No,
	];
	let (headers, best_header) = build_headers_with_epoch_flags(flags);
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 2, 1).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 5);
	assert_eq!(header_numbers(&data.descendants), vec![6, 7]);

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 3, 1).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 4);
	assert_eq!(header_numbers(&data.descendants), vec![5, 6, 7]);

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 4, 1).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 4, 2).await;
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 3);
	assert_eq!(header_numbers(&data.descendants), vec![4, 5, 6, 7]);

	let result = offset_relay_parent_find_descendants(&mut cache, best_header, 5, 2).await;
	let data = result.unwrap().unwrap();
	assert_eq!(*data.relay_parent().number(), 2);
	assert_eq!(header_numbers(&data.descendants), vec![3, 4, 5, 6, 7]);
}

#[rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[tokio::test]
async fn determine_core_new_relay_parent(#[case] n_cores: u32) {
	let (headers, _best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Create a test relay parent header
	let relay_parent = RelayHeader {
		parent_hash: Default::default(),
		number: 100,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	// Setup claim queue data for the cache
	let cores = (0..n_cores).map(CoreIndex).collect();
	cache.set_test_data(relay_parent.clone(), cores, Default::default());

	// For V1/V2 mode: claim_queue_relay_block = relay_parent.hash()
	let result = determine_cores(&mut cache, &relay_parent, 1.into(), 0).await;

	let core = result.unwrap();
	let core = core.unwrap();
	assert_eq!(core.core_info().selector, CoreSelector(0));
	assert_eq!(core.core_index(), CoreIndex(0));
	assert_eq!(core.total_cores(), n_cores);
}

#[tokio::test]
async fn determine_core_no_cores_available() {
	let (headers, _best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Create a test relay parent header
	let relay_parent = RelayHeader {
		parent_hash: Default::default(),
		number: 100,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	// Setup empty claim queue
	cache.set_test_data(relay_parent.clone(), vec![], Default::default());

	let result = determine_cores(&mut cache, &relay_parent, 1.into(), 0).await;

	let core = result.unwrap();
	assert!(core.is_none());
}

#[tokio::test]
// Only depth-0 assignments count: cores where our para appears deeper must not be returned.
async fn determine_cores_only_returns_para_assigned_cores() {
	let (headers, _best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	let relay_parent = RelayHeader {
		parent_hash: Default::default(),
		number: 100,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	let our_para = ParaId::from(1);
	let other_para = ParaId::from(2);

	// Core 0: other_para at depth 0, our_para at depth 1 — must not appear in result at offset 0.
	// Core 1: other_para at depth 0 only — must not appear.
	// Core 2: our_para at depth 0 — the only core that should be returned.
	let mut claim_queue = BTreeMap::new();
	claim_queue.insert(CoreIndex(0), VecDeque::from([other_para, our_para]));
	claim_queue.insert(CoreIndex(1), VecDeque::from([other_para]));
	claim_queue.insert(CoreIndex(2), VecDeque::from([our_para]));

	cache.set_test_data_with_claim_queue(relay_parent.clone(), claim_queue, Default::default());

	let result = determine_cores(&mut cache, &relay_parent, our_para, 0).await;
	let cores = result.unwrap().unwrap();
	assert_eq!(cores.total_cores(), 1);
	assert_eq!(cores.core_index(), CoreIndex(2));
}

#[derive(Clone)]
pub struct TestRelayClient {
	headers: HashMap<RelayHash, RelayHeader>,
	best_hash: Arc<Mutex<Option<RelayHash>>>,
	best_notifications: Arc<Mutex<Option<Pin<Box<dyn Stream<Item = RelayHeader> + Send + Sync>>>>>,
}

impl TestRelayClient {
	pub fn new(headers: HashMap<RelayHash, RelayHeader>) -> Self {
		Self {
			headers,
			best_hash: Default::default(),
			best_notifications: Arc::new(Mutex::new(None)),
		}
	}

	pub fn new_with_best(headers: HashMap<RelayHash, RelayHeader>, best_hash: RelayHash) -> Self {
		Self {
			headers,
			best_hash: Arc::new(Mutex::new(Some(best_hash))),
			best_notifications: Arc::new(Mutex::new(None)),
		}
	}

	pub fn set_best_hash(&mut self, best_hash: Option<RelayHash>) {
		*self.best_hash.lock().unwrap() = best_hash;
	}

	pub fn set_best_notifications(
		&mut self,
		best_notifications: Pin<Box<dyn Stream<Item = RelayHeader> + Send + Sync>>,
	) {
		*self.best_notifications.lock().unwrap() = Some(best_notifications);
	}
}

#[async_trait]
impl RelayChainInterface for TestRelayClient {
	async fn validators(&self, _: RelayHash) -> RelayChainResult<Vec<ValidatorId>> {
		Ok((0..N_VALIDATORS)
			.map(|i| ValidatorId::from(sr25519::Public::from_raw([i as u8; 32])))
			.collect())
	}

	async fn best_block_hash(&self) -> RelayChainResult<RelayHash> {
		self.best_hash
			.lock()
			.unwrap()
			.ok_or_else(|| RelayChainError::GenericError("No best hash set".into()))
	}
	async fn finalized_block_hash(&self) -> RelayChainResult<RelayHash> {
		unimplemented!("Not needed for test")
	}

	async fn retrieve_dmq_contents(
		&self,
		_: ParaId,
		_: RelayHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		unimplemented!("Not needed for test")
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		_: ParaId,
		_: RelayHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		unimplemented!("Not needed for test")
	}

	async fn persisted_validation_data(
		&self,
		hash: RelayHash,
		_: ParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		use cumulus_primitives_core::PersistedValidationData;

		if self.headers.get(&hash).is_none() {
			return Ok(None);
		}

		Ok(Some(PersistedValidationData {
			parent_head: Default::default(),
			relay_parent_number: 100,
			relay_parent_storage_root: Default::default(),
			max_pov_size: 1024 * 1024,
		}))
	}

	async fn validation_code_hash(
		&self,
		_: RelayHash,
		_: ParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<ValidationCodeHash>> {
		unimplemented!("Not needed for test")
	}

	async fn candidate_pending_availability(
		&self,
		_: RelayHash,
		_: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceiptV2>> {
		unimplemented!("Not needed for test")
	}

	async fn candidates_pending_availability(
		&self,
		_: RelayHash,
		_: ParaId,
	) -> RelayChainResult<Vec<CommittedCandidateReceiptV2>> {
		unimplemented!("Not needed for test")
	}

	async fn session_index_for_child(&self, _: RelayHash) -> RelayChainResult<SessionIndex> {
		Ok(SESSION_INDEX)
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		unimplemented!("Not needed for test")
	}

	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		unimplemented!("Not needed for test")
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		unimplemented!("Not needed for test")
	}

	fn overseer_handle(&self) -> RelayChainResult<OverseerHandle> {
		unimplemented!("Not needed for test")
	}

	async fn get_storage_by_key(
		&self,
		_: RelayHash,
		_: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		unimplemented!("Not needed for test")
	}

	async fn prove_read(
		&self,
		_: RelayHash,
		_: &Vec<Vec<u8>>,
	) -> RelayChainResult<sc_client_api::StorageProof> {
		unimplemented!("Not needed for test")
	}

	async fn prove_child_read(
		&self,
		_: RelayHash,
		_: &cumulus_relay_chain_interface::ChildInfo,
		_: &[Vec<u8>],
	) -> RelayChainResult<sc_client_api::StorageProof> {
		unimplemented!("Not needed for test")
	}

	async fn wait_for_block(&self, _: RelayHash) -> RelayChainResult<()> {
		unimplemented!("Not needed for test")
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		Ok(self.best_notifications.lock().unwrap().take().unwrap())
	}

	async fn header(
		&self,
		block_id: BlockId<polkadot_primitives::Block>,
	) -> RelayChainResult<Option<PHeader>> {
		let hash = match block_id {
			BlockId::Hash(hash) => hash,
			BlockId::Number(_) => unimplemented!("Not needed for test"),
		};
		let header = self.headers.get(&hash);

		Ok(header.cloned())
	}

	async fn availability_cores(
		&self,
		_relay_parent: RelayHash,
	) -> RelayChainResult<Vec<CoreState<RelayHash, BlockNumber>>> {
		unimplemented!("Not needed for test");
	}

	async fn version(&self, _: RelayHash) -> RelayChainResult<RuntimeVersion> {
		unimplemented!("Not needed for test");
	}

	async fn claim_queue(
		&self,
		_: RelayHash,
	) -> RelayChainResult<BTreeMap<CoreIndex, VecDeque<ParaId>>> {
		// Return empty claim queue for offset tests
		Ok(BTreeMap::new())
	}

	async fn call_runtime_api(
		&self,
		_method_name: &'static str,
		_hash: RelayHash,
		_payload: &[u8],
	) -> RelayChainResult<Vec<u8>> {
		unimplemented!("Not needed for test")
	}

	async fn scheduling_lookahead(&self, _: RelayHash) -> RelayChainResult<u32> {
		unimplemented!("Not needed for test")
	}

	async fn candidate_events(&self, _: RelayHash) -> RelayChainResult<Vec<CandidateEvent>> {
		unimplemented!("Not needed for test")
	}

	async fn max_relay_parent_session_age(&self, _at: RelayHash) -> RelayChainResult<u32> {
		unimplemented!("Not needed for test")
	}

	async fn node_features(&self, _at: RelayHash) -> RelayChainResult<NodeFeatures> {
		Ok(NodeFeatures::default())
	}

	async fn ancestor_relay_parent_info(
		&self,
		_at: RelayHash,
		_session_index: SessionIndex,
		_relay_parent: RelayHash,
	) -> RelayChainResult<Option<RelayParentInfo<RelayHash, BlockNumber>>> {
		unimplemented!("Not needed for test")
	}
}

/// Build a consecutive set of relay headers whose digest entries optionally carry a BABE
/// epoch-change marker, returning the underlying map and the hash of the last header.
fn build_headers_with_epoch_flags(
	flags: &[HasEpochChange],
) -> (HashMap<RelayHash, RelayHeader>, RelayHeader) {
	let mut headers = HashMap::new();
	let mut parent_hash = RelayHash::default();
	let mut last_header = RelayHeader {
		parent_hash: Default::default(),
		number: 0,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	for (index, has_epoch_change) in flags.iter().enumerate() {
		let mut digest = sp_runtime::generic::Digest::default();
		if *has_epoch_change == HasEpochChange::Yes {
			digest.push(babe_epoch_change_digest_item());
		}

		let header = RelayHeader {
			parent_hash,
			number: index as u32,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest,
		};

		let hash = header.hash();
		headers.insert(hash, header.clone());
		parent_hash = hash;
		last_header = header;
	}

	(headers, last_header)
}

/// Create a BABE `NextEpochData` digest item for use in tests.
pub fn babe_epoch_change_digest_item() -> sp_runtime::generic::DigestItem {
	let authority_id = AuthorityId::from(sr25519::Public::from_raw([1u8; 32]));
	let next_epoch =
		NextEpochDescriptor { authorities: vec![(authority_id, 1u64)], randomness: [0u8; 32] };
	let log = BabeConsensusLog::NextEpochData(next_epoch);
	sp_runtime::generic::DigestItem::Consensus(BABE_ENGINE_ID, log.encode())
}

fn create_header_chain() -> (HashMap<RelayHash, RelayHeader>, RelayHeader) {
	let mut headers = HashMap::new();
	let mut current_parent = None;
	let mut last_header = RelayHeader {
		parent_hash: Default::default(),
		number: 0,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	for number in 0..=100 {
		let mut header = RelayHeader {
			parent_hash: Default::default(),
			number,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest: Default::default(),
		};
		if let Some(hash) = current_parent {
			header.parent_hash = hash;
		}

		let header_hash = header.hash();
		headers.insert(header_hash, header.clone());
		current_parent = Some(header_hash);
		last_header = header;
	}

	(headers, last_header)
}

// Test extension for RelayChainDataCache
impl RelayChainDataCache<TestRelayClient> {
	pub fn set_test_data(
		&mut self,
		relay_parent_header: RelayHeader,
		cores: Vec<CoreIndex>,
		node_features: NodeFeatures,
	) {
		self.set_test_data_with_last_selector(relay_parent_header, cores, node_features);
	}

	fn set_test_data_with_last_selector(
		&mut self,
		relay_parent_header: RelayHeader,
		cores: Vec<CoreIndex>,
		node_features: NodeFeatures,
	) {
		let relay_parent_hash = relay_parent_header.hash();

		let mut claim_queue = BTreeMap::new();
		for core_index in cores {
			claim_queue.insert(core_index, [ParaId::from(1)].into());
		}

		let claim_queue_snapshot = ClaimQueueSnapshot::from(claim_queue);

		let data = RelayChainData {
			relay_header: relay_parent_header,
			claim_queue: claim_queue_snapshot,
			max_pov_size: 1024 * 1024,
			node_features,
		};

		self.insert_test_data(relay_parent_hash, data);
	}

	/// Build fixture data with explicit per-core para assignments, allowing mixed claim queues
	/// where different cores are assigned to different paras at each depth.
	fn set_test_data_with_claim_queue(
		&mut self,
		relay_parent_header: RelayHeader,
		claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
		node_features: NodeFeatures,
	) {
		let relay_parent_hash = relay_parent_header.hash();
		let data = RelayChainData {
			relay_header: relay_parent_header,
			claim_queue: ClaimQueueSnapshot::from(claim_queue),
			max_pov_size: 1024 * 1024,
			node_features,
		};
		self.insert_test_data(relay_parent_hash, data);
	}
}

/// Create a relay header with a BABE pre-digest containing the given slot.
pub fn relay_header_with_slot(number: u32, parent_hash: RelayHash, slot: u64) -> RelayHeader {
	use sc_consensus_babe::{CompatibleDigestItem, PreDigest, SecondaryPlainPreDigest};
	use sp_runtime::DigestItem;

	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(<DigestItem as CompatibleDigestItem>::babe_pre_digest(PreDigest::SecondaryPlain(
		SecondaryPlainPreDigest { authority_index: 0, slot: slot.into() },
	)));

	RelayHeader {
		parent_hash,
		number,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	}
}

/// Covers `anchor_claim_queue` -> [`CollatorMessage::claim_queue`] -> `SegmentDistributor`.
///
/// The claim queue the block builder captured at the scheduling anchor is the one the UMP core
/// selection is checked against, so a segment is distributed only when the collation's selected
/// core matches that claim queue — not some other queue the collation task might reach for.
mod claim_queue_plumbing {
	use super::*;
	use cumulus_client_collator::{
		metrics::Metrics, segment::SegmentDistributor, service::ServiceInterface,
	};
	use cumulus_client_consensus_common::ParachainCandidate;
	use cumulus_primitives_core::{ParachainBlockData, SchedulingProof};
	use polkadot_node_primitives::{BlockData, Collation, MaybeCompressedPoV, PoV};
	use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage, Segment};
	use polkadot_node_subsystem_util::metered::MeteredReceiver;
	use polkadot_overseer::{Event, Handle};
	use polkadot_primitives::{
		Block as PBlock, ClaimQueueOffset, HeadData, PersistedValidationData, UMPSignal,
		ValidationCodeHash, UMP_SEPARATOR,
	};
	use sp_api::StorageProof;
	use sp_trie::CompactProof;

	const OUR_PARA: u32 = 1;

	/// Hands back a fixed collation carrying `SelectCore(0, 0)`, so the core the segment is
	/// accepted on is decided entirely by the claim queue that reaches `build_segment`.
	struct MockCollatorService;

	impl ServiceInterface<PBlock> for MockCollatorService {
		fn check_block_status(&self, _: RelayHash, _: &RelayHeader) -> bool {
			true
		}

		fn build_collation(
			&self,
			_: &RelayHeader,
			_: RelayHash,
			_: ParachainCandidate<PBlock>,
			_: Option<SchedulingProof>,
		) -> Option<(Collation, ParachainBlockData<PBlock>)> {
			unimplemented!("Not needed for test")
		}

		fn build_multi_block_collation(
			&self,
			_: &RelayHeader,
			_: Vec<PBlock>,
			_: StorageProof,
			_: Option<SchedulingProof>,
		) -> Option<(Collation, ParachainBlockData<PBlock>)> {
			let mut collation = Collation {
				upward_messages: Default::default(),
				horizontal_messages: Default::default(),
				new_validation_code: None,
				head_data: HeadData(vec![1, 2, 3]),
				proof_of_validity: MaybeCompressedPoV::Raw(PoV { block_data: BlockData(vec![]) }),
				processed_downward_messages: 0,
				hrmp_watermark: 0,
			};
			collation.upward_messages.force_push(UMP_SEPARATOR);
			collation
				.upward_messages
				.force_push(UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0)).encode());

			let block_data = ParachainBlockData::new(
				Vec::new(),
				CompactProof { encoded_nodes: Vec::new() },
				None,
			);
			Some((collation, block_data))
		}

		fn announce_block(&self, _: RelayHash, _: Option<Vec<u8>>) {}
	}

	fn message(core_index: CoreIndex, claim_queue: ClaimQueueSnapshot) -> CollatorMessage<PBlock> {
		CollatorMessage {
			relay_parent: RelayHash::repeat_byte(0xAA),
			scheduling_proof: None,
			parent_header: RelayHeader {
				parent_hash: Default::default(),
				number: 1,
				state_root: Default::default(),
				extrinsics_root: Default::default(),
				digest: Default::default(),
			},
			blocks: Vec::new(),
			proof: StorageProof::empty(),
			validation_code_hash: ValidationCodeHash::from(RelayHash::repeat_byte(42)),
			core_index,
			validation_data: PersistedValidationData {
				parent_head: HeadData(vec![1, 2, 3]),
				relay_parent_number: 1,
				relay_parent_storage_root: Default::default(),
				max_pov_size: 1024 * 1024,
			},
			claim_queue,
		}
	}

	/// The same message, but scheduled at a block other than its relay parent, so the collation
	/// task has to build a V3 segment anchored at [`scheduling_anchor`].
	fn v3_message(
		core_index: CoreIndex,
		claim_queue: ClaimQueueSnapshot,
	) -> CollatorMessage<PBlock> {
		CollatorMessage {
			scheduling_proof: Some(SchedulingProof {
				header_chain: Vec::new(),
				internal_scheduling_parent_header: scheduling_anchor_header(),
				signed_scheduling_info: None,
			}),
			..message(core_index, claim_queue)
		}
	}

	/// The header the V3 scheduling proof anchors at. Distinct from the message's relay parent.
	fn scheduling_anchor_header() -> RelayHeader {
		RelayHeader {
			parent_hash: RelayHash::repeat_byte(0xBB),
			number: 7,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest: Default::default(),
		}
	}

	fn scheduling_anchor() -> RelayHash {
		scheduling_anchor_header().hash()
	}

	fn claim_queue(cores: &[u32]) -> ClaimQueueSnapshot {
		ClaimQueueSnapshot::from(
			cores
				.iter()
				.map(|core| (CoreIndex(*core), VecDeque::from([ParaId::from(OUR_PARA)])))
				.collect::<BTreeMap<_, _>>(),
		)
	}

	/// Run `handle_collation_message` and return the collator protocol messages it produced.
	async fn distributed(message: CollatorMessage<PBlock>) -> Vec<CollatorProtocolMessage> {
		let (headers, _) = create_header_chain();
		let relay_client = TestRelayClient::new(headers);
		let (tx, mut rx): (_, MeteredReceiver<Event>) =
			polkadot_node_subsystem_util::metered::channel(16);
		let mut distributor = SegmentDistributor::new(
			relay_client.clone(),
			Handle::new(tx),
			ParaId::from(OUR_PARA),
			Metrics::default(),
		);

		handle_collation_message(
			message,
			&MockCollatorService,
			&mut distributor,
			relay_client,
			None,
		)
		.await;

		let mut messages = Vec::new();
		while let Ok(Some(event)) = rx.try_next() {
			if let Event::MsgToSubsystem { msg: AllMessages::CollatorProtocol(msg), .. } = event {
				messages.push(msg);
			}
		}
		messages
	}

	/// The forwarded claim queue assigns core 2, and that is the core the segment goes out on.
	#[tokio::test]
	async fn forwarded_claim_queue_decides_the_core() {
		let messages = distributed(message(CoreIndex(2), claim_queue(&[2]))).await;

		match &messages[..] {
			[CollatorProtocolMessage::DistributeSegment { core_index, para_id, .. }] => {
				assert_eq!(*core_index, CoreIndex(2));
				assert_eq!(*para_id, ParaId::from(OUR_PARA));
			},
			other => panic!("expected exactly one `DistributeSegment`, got {}", other.len()),
		}
	}

	/// A claim queue that does not back the message's core rejects the segment. This is the case
	/// that breaks if the claim queue for some other block is used in place of the anchor's.
	#[tokio::test]
	async fn claim_queue_for_another_anchor_rejects_the_segment() {
		let messages = distributed(message(CoreIndex(2), claim_queue(&[0]))).await;

		assert!(messages.is_empty());
	}

	/// V3: the scheduling anchor is not the relay parent. The segment must be tagged with the
	/// anchor and go out on the core the anchor's claim queue backs.
	#[tokio::test]
	async fn v3_segment_is_anchored_at_the_scheduling_parent() {
		let messages = distributed(v3_message(CoreIndex(2), claim_queue(&[2]))).await;

		match &messages[..] {
			[CollatorProtocolMessage::DistributeSegment { core_index, para_id, segment }] => {
				assert_eq!(*core_index, CoreIndex(2));
				assert_eq!(*para_id, ParaId::from(OUR_PARA));
				match segment {
					Segment::V3 { scheduling_parent, scheduling_session, candidates } => {
						assert_eq!(*scheduling_parent, scheduling_anchor());
						assert_ne!(*scheduling_parent, RelayHash::repeat_byte(0xAA));
						assert_eq!(*scheduling_session, SESSION_INDEX);
						assert_eq!(candidates.len(), 1);
					},
					other => panic!("expected a V3 segment, got {other:?}"),
				}
			},
			other => panic!("expected exactly one `DistributeSegment`, got {}", other.len()),
		}
	}

	/// V3: the core is checked against the anchor's claim queue, so one that does not back it
	/// rejects the segment.
	#[tokio::test]
	async fn v3_claim_queue_for_another_anchor_rejects_the_segment() {
		let messages = distributed(v3_message(CoreIndex(2), claim_queue(&[0]))).await;

		assert!(messages.is_empty());
	}
}
