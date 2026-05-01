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
		determine_cores, offset_relay_parent_find_descendants, wait_for_current_relay_block,
	},
	relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
};
use async_trait::async_trait;
use codec::Encode;
use cumulus_primitives_core::CoreSelector;
use cumulus_relay_chain_interface::*;
use futures::Stream;
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	CandidateEvent, CommittedCandidateReceiptV2, CoreIndex, Hash as RelayHash,
	Header as RelayHeader, Id as ParaId,
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
	time::Duration,
};

#[tokio::test]
async fn offset_test_zero_offset() {
	let (headers, best_header) = create_header_chain();
	let best_hash = best_header.hash();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 0).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(data.descendants_len(), 0);
	assert_eq!(data.relay_parent().hash(), best_hash);
	assert!(data.into_inherent_descendant_list().is_empty());
}

#[tokio::test]
async fn offset_test_two_offset() {
	let (headers, best_header) = create_header_chain();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 2).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(data.descendants_len(), 2);
	assert_eq!(*data.relay_parent().number(), 98);
	let descendant_list = data.into_inherent_descendant_list();
	assert_eq!(descendant_list.len(), 3);
	assert_eq!(*descendant_list.first().unwrap().number(), 98);
	assert_eq!(*descendant_list.last().unwrap().number(), 100);
}

#[tokio::test]
async fn offset_test_five_offset() {
	let (headers, best_header) = create_header_chain();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 5).await;
	assert!(result.is_ok());
	let data = result.unwrap().unwrap();
	assert_eq!(data.descendants_len(), 5);
	assert_eq!(*data.relay_parent().number(), 95);
	let descendant_list = data.into_inherent_descendant_list();
	assert_eq!(descendant_list.len(), 6);
	assert_eq!(*descendant_list.first().unwrap().number(), 95);
	assert_eq!(*descendant_list.last().unwrap().number(), 100);
}

#[tokio::test]
async fn offset_test_too_long() {
	let (headers, best_header) = create_header_chain();

	let client = TestRelayClient::new(headers);

	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 200).await;
	assert!(result.is_err());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 101).await;
	assert!(result.is_err());
}

#[derive(PartialEq)]
enum HasEpochChange {
	Yes,
	No,
}

#[rstest]
#[case::in_best(
	&[HasEpochChange::No, HasEpochChange::No, HasEpochChange::Yes],
)]
#[case::in_first_ancestor(
	&[HasEpochChange::No, HasEpochChange::Yes, HasEpochChange::No],
)]
#[case::in_second_ancestor(
	&[HasEpochChange::Yes, HasEpochChange::No, HasEpochChange::No],
)]
#[tokio::test]
async fn offset_returns_none_when_epoch_change_encountered(#[case] flags: &[HasEpochChange]) {
	let (headers, best_header) = build_headers_with_epoch_flags(flags);
	let client = TestRelayClient::new(headers);
	let mut cache = RelayChainDataCache::new(client, 1.into());

	let result = offset_relay_parent_find_descendants(&mut cache, best_header.clone(), 3).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());
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
	cache.set_test_data(relay_parent.clone(), vec![CoreIndex(0), CoreIndex(1)]);

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
	cache.set_test_data(relay_parent.clone(), vec![]);

	let result = determine_cores(&mut cache, &relay_parent, 1.into(), 0).await;

	let core = result.unwrap();
	assert!(core.is_none());
}

#[derive(Clone)]
struct TestRelayClient {
	headers: HashMap<RelayHash, RelayHeader>,
	best_hash: std::sync::Arc<std::sync::Mutex<Option<RelayHash>>>,
}

impl TestRelayClient {
	fn new(headers: HashMap<RelayHash, RelayHeader>) -> Self {
		Self { headers, best_hash: Default::default() }
	}

	fn new_with_best(headers: HashMap<RelayHash, RelayHeader>, best_hash: RelayHash) -> Self {
		Self { headers, best_hash: std::sync::Arc::new(std::sync::Mutex::new(Some(best_hash))) }
	}

	fn set_best_hash(&self, hash: RelayHash) {
		*self.best_hash.lock().unwrap() = Some(hash);
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
		let digest = if *has_epoch_change == HasEpochChange::Yes {
			babe_epoch_change_digest()
		} else {
			Default::default()
		};

		let header = RelayHeader {
			parent_hash,
			number: (index as u32 + 1),
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

/// Create a digest containing a single BABE `NextEpochData` item for use in tests.
fn babe_epoch_change_digest() -> sp_runtime::generic::Digest {
	let mut digest = sp_runtime::generic::Digest::default();
	let authority_id = AuthorityId::from(sr25519::Public::from_raw([1u8; 32]));
	let next_epoch =
		NextEpochDescriptor { authorities: vec![(authority_id, 1u64)], randomness: [0u8; 32] };
	let log = BabeConsensusLog::NextEpochData(next_epoch);
	digest.push(sp_runtime::generic::DigestItem::Consensus(BABE_ENGINE_ID, log.encode()));
	digest
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

		let header_hash = header.hash();
		headers.insert(header_hash, header.clone());
		current_parent = Some(header_hash);
		last_header = header;
	}

	(headers, last_header)
}

// Test extension for RelayChainDataCache
impl RelayChainDataCache<TestRelayClient> {
	fn set_test_data(&mut self, relay_parent_header: RelayHeader, cores: Vec<CoreIndex>) {
		self.set_test_data_with_last_selector(relay_parent_header, cores);
	}

	fn set_test_data_with_last_selector(
		&mut self,
		relay_parent_header: RelayHeader,
		cores: Vec<CoreIndex>,
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
		};

		self.insert_test_data(relay_parent_hash, data);
	}
}

/// Create a relay header with a BABE pre-digest containing the given slot.
fn relay_header_with_slot(number: u32, parent_hash: RelayHash, slot: u64) -> RelayHeader {
	use sc_consensus_babe::{CompatibleDigestItem, PreDigest, SecondaryPlainPreDigest};
	use sp_runtime::DigestItem;

	let pre_digest = PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
		authority_index: 0,
		slot: slot.into(),
	});

	let mut digest = sp_runtime::generic::Digest::default();
	digest.push(<DigestItem as CompatibleDigestItem>::babe_pre_digest(pre_digest));

	RelayHeader {
		parent_hash,
		number,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest,
	}
}

/// Test the original bug scenario: relay block propagation exceeds `slot_offset`,
/// causing the collator to see a stale relay parent at a slot boundary.
///
/// `wait_for_current_relay_block` must block until a fresh relay block arrives
/// (via the notification stream), then return that block's hash.
#[tokio::test]
async fn wait_for_current_relay_block_waits_when_stale() {
	let relay_slot_duration = Duration::from_secs(6);
	let slot_offset = Duration::from_secs(1);

	let now_ms = super::slot_timer::duration_now().saturating_sub(slot_offset).as_millis() as u64;
	let current_slot = now_ms / relay_slot_duration.as_millis() as u64;

	// Slot 0 is always stale. A slot far in the future is always fresh.
	let stale_slot = 0;
	let fresh_slot = current_slot + 100;

	let r_stale = relay_header_with_slot(100, Default::default(), stale_slot);
	let r_stale_hash = r_stale.hash();
	let r_fresh = relay_header_with_slot(101, r_stale_hash, fresh_slot);
	let r_fresh_hash = r_fresh.hash();

	let mut headers = HashMap::new();
	headers.insert(r_stale_hash, r_stale.clone());
	headers.insert(r_fresh_hash, r_fresh.clone());

	let client = TestRelayClient::new_with_best(headers, r_stale_hash);
	let mut cache = RelayChainDataCache::new(client.clone(), 1.into());
	cache.set_test_data(r_stale, vec![]);
	cache.set_test_data(r_fresh, vec![]);

	let (tx, mut rx) = futures::channel::mpsc::unbounded::<RelayHeader>();

	let client_clone = client.clone();
	let mut handle = tokio::spawn(async move {
		wait_for_current_relay_block(
			&client_clone,
			&mut cache,
			&mut rx,
			slot_offset,
			relay_slot_duration,
		)
		.await
	});

	// The function should not return before receiving a notification — the best
	// block (slot 0) is always stale. The slot_offset timeout (1s) will fire,
	// but the function loops and waits again since the best hash hasn't changed.
	// We use a shorter timeout to verify it's still blocked.
	assert!(
		tokio::time::timeout(Duration::from_millis(100), &mut handle).await.is_err(),
		"Should be waiting for fresh relay block, not returning immediately"
	);

	// Simulate: new relay block arrives. Update best hash and send notification.
	client.set_best_hash(r_fresh_hash);
	tx.unbounded_send(relay_header_with_slot(101, r_stale_hash, fresh_slot))
		.unwrap();

	let result = tokio::time::timeout(Duration::from_secs(2), handle)
		.await
		.expect("Task should complete within timeout")
		.expect("Task should not panic");

	assert_eq!(result.map(|h| h.hash()), Some(r_fresh_hash));
}

/// When the best relay block is already current, `wait_for_current_relay_block`
/// should return immediately without waiting for any notification.
#[tokio::test]
async fn wait_for_current_relay_block_returns_immediately_when_fresh() {
	let relay_slot_duration = Duration::from_secs(6);
	let slot_offset = Duration::from_secs(1);

	// Build a relay header whose BABE slot matches "now" (so it's current).
	// We use a very large slot number so that `duration_now() - offset` maps to
	// a relay slot <= this value.
	let now_ms = super::slot_timer::duration_now().saturating_sub(slot_offset).as_millis() as u64;
	let current_slot = now_ms / relay_slot_duration.as_millis() as u64;

	let header = relay_header_with_slot(100, Default::default(), current_slot);
	let header_hash = header.hash();

	let mut headers = HashMap::new();
	headers.insert(header_hash, header.clone());

	let client = TestRelayClient::new_with_best(headers, header_hash);
	let mut cache = RelayChainDataCache::new(client.clone(), 1.into());
	cache.set_test_data(header, vec![]);

	// Create a notification stream that will never produce (no sender).
	let (_tx, mut rx) = futures::channel::mpsc::unbounded::<RelayHeader>();

	let result = tokio::time::timeout(
		Duration::from_secs(1),
		wait_for_current_relay_block(
			&client,
			&mut cache,
			&mut rx,
			slot_offset,
			relay_slot_duration,
		),
	)
	.await
	.expect("Should return immediately, not timeout");

	assert_eq!(result.map(|h| h.hash()), Some(header_hash));
}
