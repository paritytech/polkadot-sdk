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
	block_builder_task::{determine_core, offset_relay_parent_find_descendants},
	relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
};
use async_trait::async_trait;
use cumulus_primitives_core::{ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem};
use cumulus_relay_chain_interface::*;
use futures::Stream;
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	CandidateEvent, CommittedCandidateReceiptV2, CoreIndex, Hash as RelayHash,
	Header as RelayHeader, Id as ParaId,
};
use sp_runtime::{generic::BlockId, testing::Header as TestHeader, traits::Header};
use sp_version::RuntimeVersion;
use std::{
	collections::{BTreeMap, HashMap, VecDeque},
	pin::Pin,
};

#[tokio::test]
async fn offset_test_zero_offset() {
	let (headers, best_hash) = create_header_chain();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_hash, 0).await;
	assert!(result.is_ok());
	let data = result.unwrap();
	assert_eq!(data.descendants_len(), 0);
	assert_eq!(data.relay_parent().hash(), best_hash);
	assert!(data.into_inherent_descendant_list().is_empty());
}

#[tokio::test]
async fn offset_test_two_offset() {
	let (headers, best_hash) = create_header_chain();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_hash, 2).await;
	assert!(result.is_ok());
	let data = result.unwrap();
	assert_eq!(data.descendants_len(), 2);
	assert_eq!(*data.relay_parent().number(), 98);
	let descendant_list = data.into_inherent_descendant_list();
	assert_eq!(descendant_list.len(), 3);
	assert_eq!(*descendant_list.first().unwrap().number(), 98);
	assert_eq!(*descendant_list.last().unwrap().number(), 100);
}

#[tokio::test]
async fn offset_test_five_offset() {
	let (headers, best_hash) = create_header_chain();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_hash, 5).await;
	assert!(result.is_ok());
	let data = result.unwrap();
	assert_eq!(data.descendants_len(), 5);
	assert_eq!(*data.relay_parent().number(), 95);
	let descendant_list = data.into_inherent_descendant_list();
	assert_eq!(descendant_list.len(), 6);
	assert_eq!(*descendant_list.first().unwrap().number(), 95);
	assert_eq!(*descendant_list.last().unwrap().number(), 100);
}

#[tokio::test]
async fn offset_test_too_long() {
	let (headers, _best_hash) = create_header_chain();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, _best_hash, 200).await;
	assert!(result.is_err());

	let result = offset_relay_parent_find_descendants(&mut cache, _best_hash, 101).await;
	assert!(result.is_err());
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

	// Create a test para parent header at block 0 (genesis)
	let para_parent = TestHeader::new_from_number(0);

	// Setup claim queue data for the cache
	cache.set_test_data(relay_parent.clone(), vec![CoreIndex(0), CoreIndex(1)]);

	let result = determine_core(&mut cache, &relay_parent, 1.into(), &para_parent, 0).await;

	let core = result.unwrap();
	let core = core.unwrap();
	assert_eq!(core.core_selector(), CoreSelector(0));
	assert_eq!(core.core_index(), CoreIndex(0));
	assert_eq!(core.total_cores(), 2);
}

#[tokio::test]
async fn determine_core_with_core_info() {
	let (headers, best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Create a test relay parent header
	let relay_parent = RelayHeader {
		parent_hash: best_hash,
		number: 101,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	// Create a para parent header with core info in digest
	let core_info = CoreInfo {
		selector: CoreSelector(0),
		claim_queue_offset: ClaimQueueOffset(0),
		number_of_cores: 3.into(),
	};
	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(CumulusDigestItem::CoreInfo(core_info).to_digest_item());
	// Add relay parent storage root to make it a non-new relay parent
	digest.push(cumulus_primitives_core::rpsr_digest::relay_parent_storage_root_item(
		*relay_parent.state_root(),
		*relay_parent.number(),
	));

	let para_parent = TestHeader {
		parent_hash: best_hash.into(),
		number: 1,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	};

	// Setup claim queue data for the cache
	cache.set_test_data(relay_parent.clone(), vec![CoreIndex(0), CoreIndex(1), CoreIndex(2)]);

	let result = determine_core(&mut cache, &relay_parent, 1.into(), &para_parent, 0).await;

	match result {
		Ok(Some(core)) => {
			assert_eq!(core.core_selector(), CoreSelector(1)); // Should be next selector (0 + 1)
			assert_eq!(core.core_index(), CoreIndex(1));
			assert_eq!(core.total_cores(), 3);
		},
		Ok(None) => panic!("Expected Some core, got None"),
		Err(()) => panic!("determine_core returned error"),
	}
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

	// Create a test para parent header at block 0 (genesis)
	let para_parent = TestHeader::new_from_number(0);

	// Setup empty claim queue
	cache.set_test_data(relay_parent.clone(), vec![]);

	let result = determine_core(&mut cache, &relay_parent, 1.into(), &para_parent, 0).await;

	let core = result.unwrap();
	assert!(core.is_none());
}

#[tokio::test]
async fn determine_core_selector_overflow() {
	let (headers, best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Create a test relay parent header
	let relay_parent = RelayHeader {
		parent_hash: best_hash,
		number: 101,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	let core_info = CoreInfo {
		selector: CoreSelector(1),
		claim_queue_offset: ClaimQueueOffset(0),
		number_of_cores: 2.into(),
	};
	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(CumulusDigestItem::CoreInfo(core_info).to_digest_item());
	// Add relay parent storage root to make it a non-new relay parent
	digest.push(cumulus_primitives_core::rpsr_digest::relay_parent_storage_root_item(
		*relay_parent.state_root(),
		*relay_parent.number(),
	));

	let para_parent = TestHeader {
		parent_hash: best_hash.into(),
		number: 1,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	};

	// Setup claim queue with only 2 cores
	cache.set_test_data(relay_parent.clone(), vec![CoreIndex(0), CoreIndex(1)]);

	let result = determine_core(&mut cache, &relay_parent, 1.into(), &para_parent, 0).await;

	let core = result.unwrap();
	assert!(core.is_none()); // Should return None when selector overflows
}

#[tokio::test]
async fn determine_core_uses_last_claimed_core_selector() {
	let (headers, best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Create a test relay parent header
	let relay_parent = RelayHeader {
		parent_hash: best_hash,
		number: 101,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	// Create a para parent header without core info in digest (non-genesis)
	// Need to add relay parent storage root to digest to make it a non-new relay parent
	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(cumulus_primitives_core::rpsr_digest::relay_parent_storage_root_item(
		*relay_parent.state_root(),
		*relay_parent.number(),
	));

	let para_parent = TestHeader {
		parent_hash: best_hash.into(),
		number: 1,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	};

	// Setup claim queue data with last_claimed_core_selector set to 1
	cache.set_test_data_with_last_selector(
		relay_parent.clone(),
		vec![CoreIndex(0), CoreIndex(1), CoreIndex(2)],
		Some(CoreSelector(1)),
	);

	let result = determine_core(&mut cache, &relay_parent, 1.into(), &para_parent, 0).await;

	match result {
		Ok(Some(core)) => {
			// Should use last_claimed_core_selector (1) + 1 = 2
			assert_eq!(core.core_selector(), CoreSelector(2));
			assert_eq!(core.core_index(), CoreIndex(2));
			assert_eq!(core.total_cores(), 3);
		},
		Ok(None) => panic!("Expected Some core, got None"),
		Err(()) => panic!("determine_core returned error"),
	}
}

#[tokio::test]
async fn determine_core_uses_last_claimed_core_selector_wraps_around() {
	let (headers, best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Create a test relay parent header
	let relay_parent = RelayHeader {
		parent_hash: best_hash,
		number: 101,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	// Create a para parent header without core info in digest (non-genesis)
	// Need to add relay parent storage root to digest to make it a non-new relay parent
	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(cumulus_primitives_core::rpsr_digest::relay_parent_storage_root_item(
		*relay_parent.state_root(),
		*relay_parent.number(),
	));

	let para_parent = TestHeader {
		parent_hash: best_hash.into(),
		number: 1,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	};

	// Setup claim queue data with last_claimed_core_selector set to 2 (last index)
	// Next selector should wrap around to out of bounds and return None
	cache.set_test_data_with_last_selector(
		relay_parent.clone(),
		vec![CoreIndex(0), CoreIndex(1), CoreIndex(2)],
		Some(CoreSelector(2)),
	);

	let result = determine_core(&mut cache, &relay_parent, 1.into(), &para_parent, 0).await;

	match result {
		Ok(Some(_)) => panic!("Expected None due to selector overflow"),
		Ok(None) => {
			// This is expected - selector 2 + 1 = 3, but only cores 0,1,2 available
		},
		Err(()) => panic!("determine_core returned error"),
	}
}

#[tokio::test]
async fn determine_core_no_last_claimed_core_selector() {
	let (headers, best_hash) = create_header_chain();
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	// Create a test relay parent header
	let relay_parent = RelayHeader {
		parent_hash: best_hash,
		number: 101,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	};

	// Create a para parent header without core info in digest (non-genesis)
	// Need to add relay parent storage root to digest to make it a non-new relay parent
	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(cumulus_primitives_core::rpsr_digest::relay_parent_storage_root_item(
		*relay_parent.state_root(),
		*relay_parent.number(),
	));

	let para_parent = TestHeader {
		parent_hash: best_hash.into(),
		number: 1,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	};

	// Setup claim queue data with no last_claimed_core_selector (None)
	cache.set_test_data_with_last_selector(
		relay_parent.clone(),
		vec![CoreIndex(0), CoreIndex(1), CoreIndex(2)],
		None,
	);

	let result = determine_core(&mut cache, &relay_parent, 1.into(), &para_parent, 0).await;

	match result {
		Ok(Some(core)) => {
			// Should start from selector 0 + 1 = 1 when no last selector
			assert_eq!(core.core_selector(), CoreSelector(1));
			assert_eq!(core.core_index(), CoreIndex(1));
			assert_eq!(core.total_cores(), 3);
		},
		Ok(None) => panic!("Expected Some core, got None"),
		Err(()) => panic!("determine_core returned error"),
	}
}

#[derive(Clone)]
struct TestRelayClient {
	headers: HashMap<RelayHash, RelayHeader>,
}

impl TestRelayClient {
	fn new(headers: HashMap<RelayHash, RelayHeader>) -> Self {
		Self { headers }
	}
}

#[async_trait]
impl RelayChainInterface for TestRelayClient {
	async fn validators(&self, _: RelayHash) -> RelayChainResult<Vec<ValidatorId>> {
		unimplemented!("Not needed for test")
	}

	async fn best_block_hash(&self) -> RelayChainResult<RelayHash> {
		unimplemented!("Not needed for test")
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
		_: RelayHash,
		_: ParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		use cumulus_primitives_core::PersistedValidationData;
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

	async fn wait_for_block(&self, _: RelayHash) -> RelayChainResult<()> {
		unimplemented!("Not needed for test")
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		unimplemented!("Not needed for test")
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
}

fn create_header_chain() -> (HashMap<RelayHash, RelayHeader>, RelayHash) {
	let mut headers = HashMap::new();
	let mut current_parent = None;
	let mut header_hash = RelayHash::repeat_byte(0x1);

	// Create chain from highest to lowest number
	for number in 1..=100 {
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

		header_hash = header.hash();
		// Store header and update parent for next iteration
		headers.insert(header_hash, header.clone());
		current_parent = Some(header_hash);
	}

	(headers, header_hash)
}

// Test extension for RelayChainDataCache
impl RelayChainDataCache<TestRelayClient> {
	fn set_test_data(&mut self, relay_parent_header: RelayHeader, cores: Vec<CoreIndex>) {
		self.set_test_data_with_last_selector(relay_parent_header, cores, None);
	}

	fn set_test_data_with_last_selector(
		&mut self,
		relay_parent_header: RelayHeader,
		cores: Vec<CoreIndex>,
		last_claimed_core_selector: Option<CoreSelector>,
	) {
		let relay_parent_hash = relay_parent_header.hash();

		let mut claim_queue = BTreeMap::new();
		for core_index in cores {
			claim_queue.insert(core_index, [ParaId::from(1)].into());
		}

		let claim_queue_snapshot = ClaimQueueSnapshot::from(claim_queue);

		let data = RelayChainData {
			relay_parent_header,
			claim_queue: claim_queue_snapshot,
			max_pov_size: 1024 * 1024,
			last_claimed_core_selector,
		};

		self.insert_test_data(relay_parent_hash, data);
	}
}
