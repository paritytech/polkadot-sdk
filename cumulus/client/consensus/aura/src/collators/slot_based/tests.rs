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
		assemble_core_submission, determine_cores, offset_relay_parent_find_descendants,
		sp_hedge_chains, HedgeContext, HedgeMemo, V3HeaderChains, MAX_HEDGED_SIBLINGS,
	},
	relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
	scheduling::SchedulingInfo,
	CollatorMessage,
};
use async_trait::async_trait;
use codec::Encode;
use cumulus_primitives_core::{ClaimQueueOffset, CoreInfo, CoreSelector};
use cumulus_relay_chain_interface::*;
use cumulus_test_client::runtime::Block;
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
use sc_network_types::PeerId;
use sp_consensus_aura::sr25519::{
	AuthorityId as AuraAuthorityId, AuthorityPair as AuraAuthorityPair,
};
use sp_core::sr25519;
use sp_keystore::{Keystore, KeystorePtr};
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
	import_notifications:
		Arc<Mutex<Option<Pin<Box<dyn Stream<Item = RelayHeader> + Send + Sync>>>>>,
}

impl TestRelayClient {
	pub fn new(headers: HashMap<RelayHash, RelayHeader>) -> Self {
		Self {
			headers,
			best_hash: Default::default(),
			best_notifications: Arc::new(Mutex::new(None)),
			import_notifications: Arc::new(Mutex::new(None)),
		}
	}

	pub fn new_with_best(headers: HashMap<RelayHash, RelayHeader>, best_hash: RelayHash) -> Self {
		Self {
			headers,
			best_hash: Arc::new(Mutex::new(Some(best_hash))),
			best_notifications: Arc::new(Mutex::new(None)),
			import_notifications: Arc::new(Mutex::new(None)),
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

	pub fn set_import_notifications(
		&mut self,
		import_notifications: Pin<Box<dyn Stream<Item = RelayHeader> + Send + Sync>>,
	) {
		*self.import_notifications.lock().unwrap() = Some(import_notifications);
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
		// Unset unless a test installs one: siblings only matter to the hedging tests.
		match self.import_notifications.lock().unwrap().take() {
			Some(import_notifications) => Ok(import_notifications),
			None => Ok(Box::pin(futures::stream::empty())),
		}
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

/// Create a relay header with a BABE pre-digest containing the given slot.
/// Like [`relay_header_with_slot`] but claiming a BABE *primary* slot. The VRF data is dummy:
/// only the digest variant matters.
pub fn relay_header_primary_with_slot(
	number: u32,
	parent_hash: RelayHash,
	slot: u64,
) -> RelayHeader {
	use codec::Decode;
	use sc_consensus_babe::{CompatibleDigestItem, PreDigest, PrimaryPreDigest};
	use sp_core::sr25519::vrf::{VrfPreOutput, VrfProof, VrfSignature};
	use sp_runtime::DigestItem;

	let vrf_signature = VrfSignature {
		pre_output: VrfPreOutput::decode(&mut [0u8; 32].as_slice()).expect("valid preout; qed"),
		proof: VrfProof::decode(&mut [0u8; 64].as_slice()).expect("valid proof; qed"),
	};

	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(<DigestItem as CompatibleDigestItem>::babe_pre_digest(PreDigest::Primary(
		PrimaryPreDigest { authority_index: 0, slot: slot.into(), vrf_signature },
	)));

	RelayHeader {
		parent_hash,
		number,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	}
}

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

/// Wire a [`SchedulingInfo`] to an import stream carrying `imports`, the way the block builder
/// does. `base` and `extra_cached` seed the relay data cache; imports are cached on
/// [`HEDGE_CORES`]. The returned sender keeps the stream open for a later arrival.
async fn hedge_setup(
	base: &RelayHeader,
	extra_cached: &[RelayHeader],
	imports: &[RelayHeader],
) -> (
	RelayChainDataCache<TestRelayClient>,
	SchedulingInfo<TestRelayClient>,
	futures::channel::mpsc::UnboundedSender<RelayHeader>,
) {
	let mut client = TestRelayClient::new(Default::default());
	client.set_best_hash(Some(base.hash()));
	client.set_best_notifications(Box::pin(futures::stream::empty()));
	let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
	client.set_import_notifications(Box::pin(rx));

	let mut cache = RelayChainDataCache::new(client.clone(), 1.into());
	for header in extra_cached.iter().chain(std::iter::once(base)) {
		cache.set_test_data(header.clone(), vec![], v3_node_features());
	}
	for header in imports {
		cache.set_test_data(header.clone(), HEDGE_CORES.to_vec(), v3_node_features());
	}

	let mut scheduling_info = SchedulingInfo::new(Duration::from_secs(6), Duration::ZERO);
	scheduling_info.ensure_initialized(&client, &mut cache).await;

	for header in imports {
		tx.unbounded_send(header.clone()).expect("receiver is alive; qed");
	}

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
	}
}

/// Hedging is off outside V3, at `relay_parent_offset == 0` (where the scheduling parent *is* the
/// relay parent), and on a segment long enough to leave the per-core rebuild budget at zero. The
/// last case pins the fixture down, so the others cannot pass vacuously.
#[tokio::test]
async fn sp_hedge_chains_respects_its_gates() {
	let (base, chosen) = hedge_base_and_chosen();
	let mut sibling = chosen.clone();
	sibling.state_root = [7u8; 32].into();

	for (v3_enabled, offset, max_siblings, expected) in [
		(false, 1, MAX_HEDGED_SIBLINGS, 0),
		(true, 0, MAX_HEDGED_SIBLINGS, 0),
		(true, 1, 0, 0),
		(true, 1, MAX_HEDGED_SIBLINGS, 1),
	] {
		let (mut cache, mut scheduling_info, _tx) =
			hedge_setup(&base, &[], &[sibling.clone()]).await;
		let hedged = sp_hedge_chains(
			&mut cache,
			&mut scheduling_info,
			&mut HedgeMemo::default(),
			max_siblings,
			hedge_context(&chosen, base.hash(), v3_enabled, offset),
		)
		.await;

		assert_eq!(hedged.len(), expected, "v3={v3_enabled} offset={offset} budget={max_siblings}");
	}
}

/// One slot's worth of hedge evaluation on a shared memo: a sibling that only lands mid-slot is
/// picked up by the next core, an already-resolved one costs nothing to re-evaluate, and once
/// contenders outnumber the budget the cap keeps the primaries, which sort first.
#[tokio::test]
async fn sp_hedge_chains_tracks_arrivals_and_caps_contenders() {
	let (base, chosen) = hedge_base_and_chosen();
	let mut late = chosen.clone();
	late.state_root = [7u8; 32].into();
	let mut primaries: Vec<RelayHeader> = (0..=MAX_HEDGED_SIBLINGS as u8)
		.map(|i| {
			let mut header =
				relay_header_primary_with_slot(HEDGE_BASE_NUMBER + 1, base.hash(), 101);
			header.state_root = [i; 32].into();
			header
		})
		.collect();

	let (mut cache, mut scheduling_info, tx) = hedge_setup(&base, &[], &[]).await;
	for header in std::iter::once(&late).chain(primaries.iter()) {
		cache.set_test_data(header.clone(), HEDGE_CORES.to_vec(), v3_node_features());
	}
	let mut memo = HedgeMemo::default();
	let resolve = async |cache: &mut RelayChainDataCache<TestRelayClient>,
	                     info: &mut SchedulingInfo<TestRelayClient>,
	                     memo: &mut HedgeMemo| {
		sp_hedge_chains(
			cache,
			info,
			memo,
			MAX_HEDGED_SIBLINGS,
			hedge_context(&chosen, base.hash(), true, 1),
		)
		.await
	};

	// Nothing imported yet: no siblings, and no resolution budget spent.
	assert!(resolve(&mut cache, &mut scheduling_info, &mut memo).await.is_empty());
	assert!(memo.is_empty());

	// A sibling landing mid-slot is hedged by the next core, and stays memoised for the one after.
	tx.unbounded_send(late.clone()).expect("receiver is alive; qed");
	let second = resolve(&mut cache, &mut scheduling_info, &mut memo).await;
	assert_eq!(second.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(), vec![late.hash()]);
	assert_eq!(memo.len(), 1);
	assert_eq!(resolve(&mut cache, &mut scheduling_info, &mut memo).await, second);
	assert_eq!(memo.len(), 1);

	// More contenders than the budget: the cap keeps the primaries, in hash order.
	for header in &primaries {
		tx.unbounded_send(header.clone()).expect("receiver is alive; qed");
	}
	let capped = resolve(&mut cache, &mut scheduling_info, &mut memo).await;
	primaries.sort_by_key(|header| header.hash());
	assert_eq!(
		capped.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(),
		primaries.iter().take(MAX_HEDGED_SIBLINGS).map(|h| h.hash()).collect::<Vec<_>>(),
	);
}

/// The end-to-end hedge path: a sibling seen only on the import stream (a loser never becomes
/// new-best) becomes one extra scheduling proof over the same entries, sharing the chosen parent's
/// signature. Two same-height headers are dropped instead: one on a different base, one whose
/// claim queue assigns the para to another core.
#[tokio::test]
async fn import_stream_sibling_becomes_extra_scheduling_proof() {
	let (base, chosen) = hedge_base_and_chosen();
	let other_base = relay_header_with_slot(HEDGE_BASE_NUMBER, [1u8; 32].into(), 100);
	let mut sibling = chosen.clone();
	sibling.state_root = [7u8; 32].into();
	let forked = relay_header_with_slot(HEDGE_BASE_NUMBER + 1, other_base.hash(), 101);
	let mut reassigned = chosen.clone();
	reassigned.state_root = [8u8; 32].into();

	let (mut cache, mut scheduling_info, _tx) =
		hedge_setup(&base, &[other_base], &[sibling.clone(), forked, reassigned.clone()]).await;
	cache.set_test_data(reassigned, vec![CoreIndex(1)], v3_node_features());

	let hedged = sp_hedge_chains(
		&mut cache,
		&mut scheduling_info,
		&mut HedgeMemo::default(),
		MAX_HEDGED_SIBLINGS,
		hedge_context(&chosen, base.hash(), true, 1),
	)
	.await;
	assert_eq!(hedged.iter().map(|(hash, _)| *hash).collect::<Vec<_>>(), vec![sibling.hash()]);

	let keystore: KeystorePtr = Arc::new(sp_keystore::testing::MemoryKeystore::new());
	let author_pub = AuraAuthorityId::from(
		Keystore::sr25519_generate_new(
			&*keystore,
			sp_application_crypto::key_types::AURA,
			Some(&sp_keyring::Sr25519Keyring::Alice.to_seed()),
		)
		.expect("can insert key into MemoryKeystore; qed"),
	);
	let core_info = CoreInfo {
		selector: CoreSelector(0),
		claim_queue_offset: ClaimQueueOffset(0),
		number_of_cores: 1.into(),
	};
	// A bare para header in the unincluded bucket is what makes the submission non-empty.
	let unincluded = Header::new(
		1,
		Default::default(),
		Default::default(),
		Default::default(),
		Default::default(),
	);
	let message = assemble_core_submission::<Block, AuraAuthorityPair>(
		// At `relay_parent_offset == 1` a header chain is just the scheduling parent itself.
		Some(V3HeaderChains {
			chosen: vec![chosen.clone()],
			hedged: hedged.into_iter().map(|(_, chain)| chain).collect(),
		}),
		vec![unincluded],
		None,
		&core_info,
		CoreIndex(0),
		&base,
		&author_pub,
		PeerId::random(),
		&keystore,
	)
	.expect("one resubmitted header is enough to submit; qed");

	let CollatorMessage::Segment(segment) = message else {
		panic!("V3 submissions are assembled as segments");
	};
	assert_eq!(segment.scheduling_proof.scheduling_parent(), chosen.hash());
	assert_eq!(
		segment.hedged_proofs.iter().map(|p| p.scheduling_parent()).collect::<Vec<_>>(),
		vec![sibling.hash()],
	);
	assert_eq!(segment.scheduling_proof.internal_scheduling_parent_header.hash(), base.hash());
	// The signed payload does not depend on the header chain, so it is signed once and shared.
	assert_eq!(
		segment.scheduling_proof.signed_scheduling_info,
		segment.hedged_proofs[0].signed_scheduling_info,
	);
}
