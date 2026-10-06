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
	block_builder_task::{
		determine_cores, offset_relay_parent_find_descendants, resolve_hedge_chains, HedgeContext,
		MAX_HEDGED_SIBLINGS,
	},
	relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
	scheduling::SchedulingInfo,
};
use async_trait::async_trait;
use codec::Encode;
use cumulus_primitives_aura::Slot;
use cumulus_primitives_core::CoreSelector;
use cumulus_relay_chain_interface::*;
use futures::Stream;
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	node_features::FeatureIndex, vstaging::RelayParentInfo, CandidateEvent,
	CommittedCandidateReceiptV2, CoreIndex, Hash as RelayHash, Header as RelayHeader, Id as ParaId,
	NodeFeatures,
};
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
	time::Duration,
};

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

#[tokio::test]
async fn determine_core_new_relay_parent() {
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
	cache.set_test_data(relay_parent.clone(), vec![CoreIndex(0), CoreIndex(1)], Default::default());

	// For V1/V2 mode: claim_queue_relay_block = relay_parent.hash()
	let result = determine_cores(&mut cache, &relay_parent, 1.into(), 0).await;

	let core = result.unwrap();
	let core = core.unwrap();
	assert_eq!(core.core_info().selector, CoreSelector(0));
	assert_eq!(core.core_index(), CoreIndex(0));
	assert_eq!(core.total_cores(), 2);
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
		unimplemented!("Not needed for test")
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
		unimplemented!("Not needed for test")
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		// Tests needing imports install a stream with `with_import_stream` instead.
		Ok(Box::pin(futures::stream::empty()))
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
}

/// Like [`relay_header_with_slot`] but claiming a BABE *primary* slot. The VRF data is dummy:
/// only the digest variant matters.
pub fn relay_header_primary_with_slot(
	number: u32,
	parent_hash: RelayHash,
	slot: u64,
) -> RelayHeader {
	use codec::Decode;
	use sc_consensus_babe::{CompatibleDigestItem, PreDigest, PrimaryPreDigest};
	use sp_core::sr25519::vrf::VrfSignature;
	use sp_runtime::DigestItem;

	let vrf_signature =
		VrfSignature::decode(&mut [0u8; 96].as_slice()).expect("any 96 bytes decode; qed");
	let mut header = relay_header_with_slot(number, parent_hash, slot);
	header.digest.logs =
		vec![<DigestItem as CompatibleDigestItem>::babe_pre_digest(PreDigest::Primary(
			PrimaryPreDigest { authority_index: 0, slot: slot.into(), vrf_signature },
		))];
	header
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

/// Relay height of the shared relay parent the hedging tests fork on, one below the siblings.
const HEDGE_BASE_NUMBER: u32 = 10;

/// The single core the hedging tests plan their slot on; a sibling must agree on it.
const HEDGE_CORES: [CoreIndex; 1] = [CoreIndex(0)];

pub fn v3_node_features() -> NodeFeatures {
	let mut node_features = NodeFeatures::from_vec(vec![0; 5]);
	node_features.set(FeatureIndex::CandidateReceiptV3 as usize, true);
	node_features
}

/// The relay parent the hedging tests execute against, and the scheduling parent one block above
/// it. Siblings of the latter are same-slot twins, the only class worth hedging.
fn hedge_base_and_chosen() -> (RelayHeader, RelayHeader) {
	let base = relay_header_with_slot(HEDGE_BASE_NUMBER, Default::default(), 100);
	let chosen = relay_header_with_slot(HEDGE_BASE_NUMBER + 1, base.hash(), 101);
	(base, chosen)
}

/// A [`SchedulingInfo`] wired to an import stream, with the sender that feeds it.
pub fn with_import_stream(
) -> (SchedulingInfo<TestRelayClient>, futures::channel::mpsc::UnboundedSender<RelayHeader>) {
	let mut scheduling_info = SchedulingInfo::new(Duration::from_secs(6), Duration::ZERO);
	let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
	let stream: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> = Box::pin(rx);
	scheduling_info.import_notifications = futures::StreamExt::fuse(stream);
	(scheduling_info, tx)
}

/// A relay data cache holding `base` and the `imports` (assigned [`HEDGE_CORES`]), plus a
/// [`SchedulingInfo`] whose import stream already carries `imports` and stays open.
fn hedge_setup(
	base: &RelayHeader,
	imports: &[RelayHeader],
) -> (
	RelayChainDataCache<TestRelayClient>,
	SchedulingInfo<TestRelayClient>,
	futures::channel::mpsc::UnboundedSender<RelayHeader>,
) {
	let mut cache = RelayChainDataCache::new(TestRelayClient::new(Default::default()), 1.into());
	cache.set_test_data(base.clone(), vec![], v3_node_features());
	let (mut scheduling_info, tx) = with_import_stream();
	for header in imports {
		cache.set_test_data(header.clone(), HEDGE_CORES.to_vec(), v3_node_features());
		tx.unbounded_send(header.clone()).expect("receiver is alive; qed");
	}
	scheduling_info.drain_imports();
	(cache, scheduling_info, tx)
}

fn hedge_context<'a>(
	scheduling_parent_header: &'a RelayHeader,
	relay_parent_hash: RelayHash,
	v3_enabled: bool,
	relay_parent_offset: u32,
) -> HedgeContext<'a> {
	HedgeContext {
		v3_enabled,
		relay_parent_offset,
		max_relay_parent_session_age: 0,
		scheduling_parent_header,
		relay_parent_hash,
		para_id: ParaId::from(1),
		claim_queue_offset: 0,
		core_indices: &HEDGE_CORES,
		// `hedge_base_and_chosen` puts `chosen` at BABE slot 101.
		production_slot: Slot::from(102),
	}
}

/// Hedging is off outside V3 and at `relay_parent_offset == 0` (where the scheduling parent *is*
/// the relay parent). The last case pins the fixture down, so the others cannot pass vacuously.
#[tokio::test]
async fn sp_hedge_chains_respects_its_gates() {
	let (base, chosen) = hedge_base_and_chosen();
	let mut sibling = chosen.clone();
	sibling.state_root = [7u8; 32].into();

	for (v3_enabled, offset, expected) in [(false, 1, 0), (true, 0, 0), (true, 1, 1)] {
		let (mut cache, mut scheduling_info, _tx) = hedge_setup(&base, &[sibling.clone()]);
		let hedged = resolve_hedge_chains(
			&mut cache,
			&mut scheduling_info,
			&hedge_context(&chosen, base.hash(), v3_enabled, offset),
		)
		.await;

		assert_eq!(hedged.len(), expected, "v3={v3_enabled} offset={offset}");
	}
}

/// Once siblings outnumber `MAX_HEDGED_SIBLINGS` the cap keeps the primaries, in hash order.
#[tokio::test]
async fn sp_hedge_chains_caps_siblings() {
	let (base, chosen) = hedge_base_and_chosen();
	let mut primaries: Vec<RelayHeader> = (0..=MAX_HEDGED_SIBLINGS as u8)
		.map(|i| {
			let mut header =
				relay_header_primary_with_slot(HEDGE_BASE_NUMBER + 1, base.hash(), 101);
			header.state_root = [i; 32].into();
			header
		})
		.collect();

	let (mut cache, mut scheduling_info, _tx) = hedge_setup(&base, &primaries);
	let capped = resolve_hedge_chains(
		&mut cache,
		&mut scheduling_info,
		&hedge_context(&chosen, base.hash(), true, 1),
	)
	.await;
	primaries.sort_by_key(|header| header.hash());
	assert_eq!(
		capped.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
		primaries.iter().take(MAX_HEDGED_SIBLINGS).map(|h| h.hash()).collect::<Vec<_>>(),
	);
}

/// A sibling seen only on the import stream (a loser never becomes new-best) is the only one
/// accepted as a hedge; two same-height headers are dropped instead: one on a different base, one
/// whose claim queue assigns the para to another core.
#[tokio::test]
async fn import_stream_sibling_is_accepted_as_hedge() {
	let (base, chosen) = hedge_base_and_chosen();
	let other_base = relay_header_with_slot(HEDGE_BASE_NUMBER, [1u8; 32].into(), 100);
	let mut sibling = chosen.clone();
	sibling.state_root = [7u8; 32].into();
	let forked = relay_header_with_slot(HEDGE_BASE_NUMBER + 1, other_base.hash(), 101);
	let mut reassigned = chosen.clone();
	reassigned.state_root = [8u8; 32].into();

	let (mut cache, mut scheduling_info, _tx) =
		hedge_setup(&base, &[sibling.clone(), forked, reassigned.clone()]);
	cache.set_test_data(other_base, vec![], v3_node_features());
	cache.set_test_data(reassigned, vec![CoreIndex(1)], v3_node_features());

	let hedged = resolve_hedge_chains(
		&mut cache,
		&mut scheduling_info,
		&hedge_context(&chosen, base.hash(), true, 1),
	)
	.await;
	assert_eq!(hedged.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(), vec![sibling.hash()]);
}
