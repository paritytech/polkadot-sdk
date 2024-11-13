// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::*;

use async_trait::async_trait;
use codec::Encode;
use cumulus_client_pov_recovery::RecoveryKind;
use cumulus_primitives_core::{
	relay_chain::{vstaging::CoreState, BlockId, BlockNumber},
	CumulusDigestItem, InboundDownwardMessage, InboundHrmpMessage,
};
use cumulus_relay_chain_interface::{
	CommittedCandidateReceipt, CoreIndex, OccupiedCoreAssumption, OverseerHandle, PHeader, ParaId,
	RelayChainInterface, RelayChainResult, SessionIndex, StorageValue, ValidatorId,
};
use cumulus_test_client::{
	runtime::{Block, Hash, Header},
	Backend, Client, InitBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use futures::{channel::mpsc, executor::block_on, select, FutureExt, Stream, StreamExt};
use futures_timer::Delay;
use polkadot_primitives::HeadData;
use sc_client_api::{Backend as _, UsageProvider};
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy};
use sp_blockchain::Backend as BlockchainBackend;
use sp_consensus::{BlockOrigin, BlockStatus};
use sp_version::RuntimeVersion;
use std::{
	collections::{BTreeMap, HashMap, VecDeque},
	pin::Pin,
	sync::{Arc, Mutex},
	time::Duration,
};

fn relay_block_num_from_hash(hash: &PHash) -> BlockNumber {
	hash.to_low_u64_be() as u32
}

fn relay_hash_from_block_num(block_number: BlockNumber) -> PHash {
	PHash::from_low_u64_be(block_number as u64)
}

struct RelaychainInner {
	new_best_heads: Option<mpsc::UnboundedReceiver<Header>>,
	finalized_heads: Option<mpsc::UnboundedReceiver<Header>>,
	new_best_heads_sender: mpsc::UnboundedSender<Header>,
	finalized_heads_sender: mpsc::UnboundedSender<Header>,
	relay_chain_hash_to_header: HashMap<PHash, Header>,
	relay_chain_hash_to_header_pending: HashMap<PHash, Header>,
}

impl RelaychainInner {
	fn new() -> Self {
		let (new_best_heads_sender, new_best_heads) = mpsc::unbounded();
		let (finalized_heads_sender, finalized_heads) = mpsc::unbounded();

		Self {
			new_best_heads_sender,
			finalized_heads_sender,
			new_best_heads: Some(new_best_heads),
			finalized_heads: Some(finalized_heads),
			relay_chain_hash_to_header: Default::default(),
			relay_chain_hash_to_header_pending: Default::default(),
		}
	}
}

#[derive(Clone)]
struct Relaychain {
	inner: Arc<Mutex<RelaychainInner>>,
}

impl Relaychain {
	fn new() -> Self {
		Self { inner: Arc::new(Mutex::new(RelaychainInner::new())) }
	}
}

#[async_trait]
impl RelayChainInterface for Relaychain {
	async fn validators(&self, _: PHash) -> RelayChainResult<Vec<ValidatorId>> {
		unimplemented!("Not needed for test")
	}

	async fn best_block_hash(&self) -> RelayChainResult<PHash> {
		unimplemented!("Not needed for test")
	}

	async fn finalized_block_hash(&self) -> RelayChainResult<PHash> {
		unimplemented!("Not needed for test")
	}

	async fn retrieve_dmq_contents(
		&self,
		_: ParaId,
		_: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		unimplemented!("Not needed for test")
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		_: ParaId,
		_: PHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		unimplemented!("Not needed for test")
	}

	async fn persisted_validation_data(
		&self,
		hash: PHash,
		_: ParaId,
		assumption: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		let inner = self.inner.lock().unwrap();
		let relay_to_header = match assumption {
			OccupiedCoreAssumption::Included => &inner.relay_chain_hash_to_header_pending,
			_ => &inner.relay_chain_hash_to_header,
		};
		let Some(parent_head) = relay_to_header.get(&hash).map(|head| head.encode().into()) else {
			return Ok(None)
		};
		Ok(Some(PersistedValidationData { parent_head, ..Default::default() }))
	}

	async fn validation_code_hash(
		&self,
		_: PHash,
		_: ParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<ValidationCodeHash>> {
		unimplemented!("Not needed for test")
	}

	async fn candidate_pending_availability(
		&self,
		_: PHash,
		_: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		unimplemented!("Not needed for test")
	}

	async fn candidates_pending_availability(
		&self,
		_: PHash,
		_: ParaId,
	) -> RelayChainResult<Vec<CommittedCandidateReceipt>> {
		unimplemented!("Not needed for test")
	}

	async fn session_index_for_child(&self, _: PHash) -> RelayChainResult<SessionIndex> {
		Ok(0)
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		unimplemented!("Not needed for test")
	}

	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let inner = self.inner.clone();
		Ok(self
			.inner
			.lock()
			.unwrap()
			.finalized_heads
			.take()
			.unwrap()
			.map(move |h| {
				// Let's abuse the "parachain header" directly as relay chain header.
				inner.lock().unwrap().relay_chain_hash_to_header.insert(h.hash(), h.clone());
				h
			})
			.boxed())
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		Ok(false)
	}

	fn overseer_handle(&self) -> RelayChainResult<OverseerHandle> {
		unimplemented!("Not needed for test")
	}

	async fn get_storage_by_key(
		&self,
		_: PHash,
		_: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		unimplemented!("Not needed for test")
	}

	async fn prove_read(
		&self,
		_: PHash,
		_: &Vec<Vec<u8>>,
	) -> RelayChainResult<sc_client_api::StorageProof> {
		unimplemented!("Not needed for test")
	}

	async fn wait_for_block(&self, _: PHash) -> RelayChainResult<()> {
		Ok(())
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let inner = self.inner.clone();
		Ok(self
			.inner
			.lock()
			.unwrap()
			.new_best_heads
			.take()
			.unwrap()
			.map(move |h| {
				// Let's abuse the "parachain header" directly as relay chain header.
				inner.lock().unwrap().relay_chain_hash_to_header.insert(h.hash(), h.clone());
				h
			})
			.boxed())
	}

	async fn header(&self, block_id: BlockId) -> RelayChainResult<Option<PHeader>> {
		let number = match block_id {
			BlockId::Hash(hash) => relay_block_num_from_hash(&hash),
			BlockId::Number(block_number) => block_number,
		};
		let parent_hash = number
			.checked_sub(1)
			.map(relay_hash_from_block_num)
			.unwrap_or_else(|| PHash::zero());

		Ok(Some(PHeader {
			parent_hash,
			number,
			digest: sp_runtime::Digest::default(),
			state_root: PHash::zero(),
			extrinsics_root: PHash::zero(),
		}))
	}

	async fn availability_cores(
		&self,
		_relay_parent: PHash,
	) -> RelayChainResult<Vec<CoreState<PHash, BlockNumber>>> {
		unimplemented!("Not needed for test");
	}

	async fn version(&self, _: PHash) -> RelayChainResult<RuntimeVersion> {
		unimplemented!("Not needed for test")
	}

	async fn claim_queue(
		&self,
		_: PHash,
	) -> RelayChainResult<BTreeMap<CoreIndex, VecDeque<ParaId>>> {
		unimplemented!("Not needed for test");
	}

	async fn call_runtime_api(
		&self,
		_method_name: &'static str,
		_hash: PHash,
		_payload: &[u8],
	) -> RelayChainResult<Vec<u8>> {
		unimplemented!("Not needed for test")
	}
}

fn sproof_with_best_parent(client: &Client) -> RelayStateSproofBuilder {
	let best_hash = client.chain_info().best_hash;
	sproof_with_parent_by_hash(client, best_hash)
}

fn sproof_with_parent_by_hash(client: &Client, hash: PHash) -> RelayStateSproofBuilder {
	let header = client.header(hash).ok().flatten().expect("No header for parent block");
	sproof_with_parent(HeadData(header.encode()))
}

fn sproof_with_parent(parent: HeadData) -> RelayStateSproofBuilder {
	let mut x = RelayStateSproofBuilder::default();
	x.para_id = cumulus_test_client::runtime::PARACHAIN_ID.into();
	x.included_para_head = Some(parent);

	x
}

fn build_block<B: InitBlockBuilder>(
	builder: &B,
	sproof: RelayStateSproofBuilder,
	at: Option<Hash>,
	timestamp: Option<u64>,
	relay_parent: Option<PHash>,
) -> Block {
	let cumulus_test_client::BlockBuilderAndSupportData { block_builder, .. } = match at {
		Some(at) => match timestamp {
			Some(ts) => builder.init_block_builder_with_timestamp(at, None, sproof, ts),
			None => builder.init_block_builder_at(at, None, sproof),
		},
		None => builder.init_block_builder(None, sproof),
	};

	let mut block = block_builder.build().unwrap().block;

	if let Some(relay_parent) = relay_parent {
		block
			.header
			.digest
			.push(CumulusDigestItem::RelayParent(relay_parent).to_digest_item());
	} else {
		// Simulate some form of post activity (like a Seal or Other generic things).
		// This is mostly used to exercise the `LevelMonitor` correct behavior.
		// (in practice we want that header post-hash != pre-hash)
		block.header.digest.push(sp_runtime::DigestItem::Other(vec![1, 2, 3]));
	}

	block
}

async fn import_block<I: BlockImport<Block>>(
	importer: &I,
	block: Block,
	origin: BlockOrigin,
	import_as_best: bool,
) {
	let (mut header, body) = block.deconstruct();

	let post_digest =
		header.digest.pop().expect("post digested is present in manually crafted block");

	let mut block_import_params = BlockImportParams::new(origin, header);
	block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(import_as_best));
	block_import_params.body = Some(body);
	block_import_params.post_digests.push(post_digest);

	importer.import_block(block_import_params).await.unwrap();
}

fn import_block_sync<I: BlockImport<Block>>(
	importer: &mut I,
	block: Block,
	origin: BlockOrigin,
	import_as_best: bool,
) {
	block_on(import_block(importer, block, origin, import_as_best));
}

fn build_and_import_block_ext<I: BlockImport<Block>>(
	client: &Client,
	origin: BlockOrigin,
	import_as_best: bool,
	importer: &mut I,
	at: Option<Hash>,
	timestamp: Option<u64>,
	relay_parent: Option<PHash>,
) -> Block {
	let sproof = match at {
		None => sproof_with_best_parent(client),
		Some(at) => sproof_with_parent_by_hash(client, at),
	};

	let block = build_block(client, sproof, at, timestamp, relay_parent);
	import_block_sync(importer, block.clone(), origin, import_as_best);
	block
}

fn build_and_import_block(mut client: Arc<Client>, import_as_best: bool) -> Block {
	build_and_import_block_ext(
		&client.clone(),
		BlockOrigin::Own,
		import_as_best,
		&mut client,
		None,
		None,
		None,
	)
}

#[test]
fn follow_new_best_works() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::default().build());

	let block = build_and_import_block(client.clone(), false);
	let relay_chain = Relaychain::new();
	let new_best_heads_sender = relay_chain.inner.lock().unwrap().new_best_heads_sender.clone();

	let consensus =
		run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}), None);

	let work = async move {
		new_best_heads_sender.unbounded_send(block.header().clone()).unwrap();
		loop {
			Delay::new(Duration::from_millis(100)).await;
			if block.hash() == client.usage_info().chain.best_hash {
				break
			}
		}
	};

	block_on(async move {
		futures::pin_mut!(consensus);
		futures::pin_mut!(work);

		select! {
			r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
			_ = work.fuse() => {},
		}
	});
}

#[test]
fn follow_new_best_with_dummy_recovery_works() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::default().build());

	let relay_chain = Relaychain::new();
	let new_best_heads_sender = relay_chain.inner.lock().unwrap().new_best_heads_sender.clone();

	let (recovery_chan_tx, mut recovery_chan_rx) = futures::channel::mpsc::channel(3);

	let consensus = run_parachain_consensus(
		100.into(),
		client.clone(),
		relay_chain,
		Arc::new(|_, _| {}),
		Some(recovery_chan_tx),
	);

	let sproof = {
		let best = client.chain_info().best_hash;
		let header = client.header(best).ok().flatten().expect("No header for best");
		sproof_with_parent(HeadData(header.encode()))
	};
	let block = build_block(&*client, sproof, None, None, None);
	let block_clone = block.clone();
	let client_clone = client.clone();

	let work = async move {
		new_best_heads_sender.unbounded_send(block.header().clone()).unwrap();
		loop {
			Delay::new(Duration::from_millis(100)).await;
			match client.block_status(block.hash()).unwrap() {
				BlockStatus::Unknown => {},
				status => {
					assert_eq!(block.hash(), client.usage_info().chain.best_hash);
					assert_eq!(status, BlockStatus::InChainWithState);
					break
				},
			}
		}
	};

	let dummy_block_recovery = async move {
		loop {
			if let Some(req) = recovery_chan_rx.next().await {
				assert_eq!(req.hash, block_clone.hash());
				assert_eq!(req.kind, RecoveryKind::Full);
				Delay::new(Duration::from_millis(500)).await;
				import_block(&mut &*client_clone, block_clone.clone(), BlockOrigin::Own, true)
					.await;
			}
		}
	};

	block_on(async move {
		futures::pin_mut!(consensus);
		futures::pin_mut!(work);

		select! {
			r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
			_ = dummy_block_recovery.fuse() => {},
			_ = work.fuse() => {},
		}
	});
}

#[test]
fn follow_finalized_works() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::default().build());

	let block = build_and_import_block(client.clone(), false);
	let relay_chain = Relaychain::new();
	let finalized_sender = relay_chain.inner.lock().unwrap().finalized_heads_sender.clone();

	let consensus =
		run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}), None);

	let work = async move {
		finalized_sender.unbounded_send(block.header().clone()).unwrap();
		loop {
			Delay::new(Duration::from_millis(100)).await;
			if block.hash() == client.usage_info().chain.finalized_hash {
				break
			}
		}
	};

	block_on(async move {
		futures::pin_mut!(consensus);
		futures::pin_mut!(work);

		select! {
			r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
			_ = work.fuse() => {},
		}
	});
}

#[test]
fn follow_finalized_does_not_stop_on_unknown_block() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::default().build());

	let block = build_and_import_block(client.clone(), false);

	let unknown_block = {
		let sproof = sproof_with_parent_by_hash(&client, block.hash());
		let block_builder = client.init_block_builder_at(block.hash(), None, sproof).block_builder;
		block_builder.build().unwrap().block
	};

	let relay_chain = Relaychain::new();
	let finalized_sender = relay_chain.inner.lock().unwrap().finalized_heads_sender.clone();

	let consensus =
		run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}), None);

	let work = async move {
		for _ in 0..3usize {
			finalized_sender.unbounded_send(unknown_block.header().clone()).unwrap();

			Delay::new(Duration::from_millis(100)).await;
		}

		finalized_sender.unbounded_send(block.header().clone()).unwrap();
		loop {
			Delay::new(Duration::from_millis(100)).await;
			if block.hash() == client.usage_info().chain.finalized_hash {
				break
			}
		}
	};

	block_on(async move {
		futures::pin_mut!(consensus);
		futures::pin_mut!(work);

		select! {
			r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
			_ = work.fuse() => {},
		}
	});
}

// It can happen that we first import a relay chain block, while not yet having the parachain
// block imported that would be set to the best block. We need to make sure to import this
// block as new best block in the moment it is imported.
#[test]
fn follow_new_best_sets_best_after_it_is_imported() {
	sp_tracing::try_init_simple();

	let client = Arc::new(TestClientBuilder::default().build());

	let block = build_and_import_block(client.clone(), false);

	let unknown_block = {
		let sproof = sproof_with_parent_by_hash(&client, block.hash());
		let block_builder = client.init_block_builder_at(block.hash(), None, sproof).block_builder;
		block_builder.build().unwrap().block
	};

	let relay_chain = Relaychain::new();
	let new_best_heads_sender = relay_chain.inner.lock().unwrap().new_best_heads_sender.clone();

	let consensus =
		run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}), None);

	let work = async move {
		new_best_heads_sender.unbounded_send(block.header().clone()).unwrap();

		loop {
			Delay::new(Duration::from_millis(100)).await;
			if block.hash() == client.usage_info().chain.best_hash {
				break
			}
		}

		// Announce the unknown block
		new_best_heads_sender.unbounded_send(unknown_block.header().clone()).unwrap();

		// Do some iterations. As this is a local task executor, only one task can run at a time.
		// Meaning that it should already have processed the unknown block.
		for _ in 0..3usize {
			Delay::new(Duration::from_millis(100)).await;
		}

		let (header, body) = unknown_block.clone().deconstruct();

		let mut block_import_params = BlockImportParams::new(BlockOrigin::Own, header);
		block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(false));
		block_import_params.body = Some(body);

		// Now import the unknown block to make it "known"
		client.import_block(block_import_params).await.unwrap();

		loop {
			Delay::new(Duration::from_millis(100)).await;
			if unknown_block.hash() == client.usage_info().chain.best_hash {
				break
			}
		}
	};

	block_on(async move {
		futures::pin_mut!(consensus);
		futures::pin_mut!(work);

		select! {
			r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
			_ = work.fuse() => {},
		}
	});
}

/// When we import a new best relay chain block, we extract the best parachain block from it and set
/// it. This works when we follow the relay chain and parachain at the tip of each other, but there
/// can be race conditions when we are doing a full sync of both or just the relay chain.
/// The problem is that we import parachain blocks as best as long as we are in major sync. So, we
/// could import block 100 as best and then import a relay chain block that says that block 99 is
/// the best parachain block. This should not happen, we should never set the best block to a lower
/// block number.
#[test]
fn do_not_set_best_block_to_older_block() {
	const NUM_BLOCKS: usize = 4;

	sp_tracing::try_init_simple();

	let backend = Arc::new(Backend::new_test(1000, 1));

	let client = Arc::new(TestClientBuilder::with_backend(backend).build());

	let blocks = (0..NUM_BLOCKS)
		.map(|_| build_and_import_block(client.clone(), true))
		.collect::<Vec<_>>();

	assert_eq!(NUM_BLOCKS as u32, client.usage_info().chain.best_number);

	let relay_chain = Relaychain::new();
	let new_best_heads_sender = relay_chain.inner.lock().unwrap().new_best_heads_sender.clone();

	let consensus =
		run_parachain_consensus(100.into(), client.clone(), relay_chain, Arc::new(|_, _| {}), None);

	let work = async move {
		new_best_heads_sender
			.unbounded_send(blocks[NUM_BLOCKS - 2].header().clone())
			.unwrap();
		// Wait for it to be processed.
		Delay::new(Duration::from_millis(300)).await;
	};

	block_on(async move {
		futures::pin_mut!(consensus);
		futures::pin_mut!(work);

		select! {
			r = consensus.fuse() => panic!("Consensus should not end: {:?}", r),
			_ = work.fuse() => {},
		}
	});

	// Build and import a new best block.
	build_and_import_block(client, true);
}

#[test]
fn prune_blocks_on_level_overflow() {
	// Here we are using the timestamp value to generate blocks with different hashes.
	const LEVEL_LIMIT: usize = 3;

	let mut ts_producer = std::iter::successors(Some(0), |&x| Some(x + 6000));
	let backend = Arc::new(Backend::new_test(1000, 3));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import = ParachainBlockImport::new_with_limit(
		client.clone(),
		backend.clone(),
		LevelLimit::Some(LEVEL_LIMIT),
	);

	let best_hash = client.chain_info().best_hash;
	let block0 = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		Some(best_hash),
		ts_producer.next(),
		None,
	);
	let id0 = block0.header.hash();

	let blocks1 = (0..LEVEL_LIMIT)
		.map(|i| {
			build_and_import_block_ext(
				&client,
				if i == 1 { BlockOrigin::NetworkInitialSync } else { BlockOrigin::Own },
				i == 1,
				&mut para_import,
				Some(id0),
				ts_producer.next(),
				None,
			)
		})
		.collect::<Vec<_>>();
	let id10 = blocks1[0].header.hash();

	let blocks2 = (0..2)
		.map(|_| {
			build_and_import_block_ext(
				&client,
				BlockOrigin::Own,
				false,
				&mut para_import,
				Some(id10),
				ts_producer.next(),
				None,
			)
		})
		.collect::<Vec<_>>();

	// Initial scenario (with B11 imported as best)
	//
	//   B0 --+-- B10 --+-- B20
	//        +-- B11   +-- B21
	//        +-- B12

	let leaves = backend.blockchain().leaves().unwrap();
	let mut expected = vec![
		blocks2[0].header.hash(),
		blocks2[1].header.hash(),
		blocks1[1].header.hash(),
		blocks1[2].header.hash(),
	];
	assert_eq!(leaves, expected);
	let best = client.usage_info().chain.best_hash;
	assert_eq!(best, blocks1[1].header.hash());

	let block13 = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		false,
		&mut para_import,
		Some(id0),
		ts_producer.next(),
		None,
	);

	// Expected scenario
	//
	//   B0 --+-- B10 --+-- B20
	//        +-- B11   +-- B21
	//        +--(B13)              <-- B12 has been replaced

	let leaves = backend.blockchain().leaves().unwrap();
	expected[3] = block13.header.hash();
	assert_eq!(leaves, expected);

	let block14 = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		false,
		&mut para_import,
		Some(id0),
		ts_producer.next(),
		None,
	);

	// Expected scenario
	//
	//   B0 --+--(B14)              <-- B10 has been replaced
	//        +-- B11
	//        +--(B13)

	let leaves = backend.blockchain().leaves().unwrap();
	expected.remove(0);
	expected.remove(0);
	expected.push(block14.header.hash());
	assert_eq!(leaves, expected);
}

#[test]
fn restore_limit_monitor() {
	// Here we are using the timestamp value to generate blocks with different hashes.
	const LEVEL_LIMIT: usize = 2;
	// Iterator that produces a new timestamp in the next slot
	let mut ts_producer = std::iter::successors(Some(0), |&x| Some(x + 6000));
	let backend = Arc::new(Backend::new_test(1000, 3));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());

	// Start with a block import not enforcing any limit...
	let mut para_import = ParachainBlockImport::new_with_limit(
		client.clone(),
		backend.clone(),
		LevelLimit::Some(usize::MAX),
	);

	let best_hash = client.chain_info().best_hash;
	let block00 = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		Some(best_hash),
		ts_producer.next(),
		None,
	);
	let id00 = block00.header.hash();

	let blocks1 = (0..LEVEL_LIMIT + 1)
		.map(|i| {
			build_and_import_block_ext(
				&client,
				if i == 1 { BlockOrigin::NetworkInitialSync } else { BlockOrigin::Own },
				i == 1,
				&mut para_import,
				Some(id00),
				ts_producer.next(),
				None,
			)
		})
		.collect::<Vec<_>>();
	let id10 = blocks1[0].header.hash();

	for _ in 0..LEVEL_LIMIT {
		build_and_import_block_ext(
			&client,
			BlockOrigin::Own,
			false,
			&mut para_import,
			Some(id10),
			ts_producer.next(),
			None,
		);
	}

	// Scenario before limit application (with B11 imported as best)
	// Import order (freshness): B00, B10, B11, B12, B20, B21
	//
	//   B00 --+-- B10 --+-- B20
	//         |         +-- B21
	//         +-- B11
	//         |
	//         +-- B12

	// Simulate a restart by forcing a new monitor structure instance

	let mut para_import = ParachainBlockImport::new_with_limit(
		client.clone(),
		backend.clone(),
		LevelLimit::Some(LEVEL_LIMIT),
	);

	let monitor_sd = para_import.monitor.clone().unwrap();

	let monitor = monitor_sd.shared_data();
	assert_eq!(monitor.import_counter, 3);
	std::mem::drop(monitor);

	let block13 = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		false,
		&mut para_import,
		Some(id00),
		ts_producer.next(),
		None,
	);

	// Expected scenario
	//
	//   B0 --+-- B11
	//        +--(B13)

	let leaves = backend.blockchain().leaves().unwrap();
	let expected = vec![blocks1[1].header.hash(), block13.header.hash()];
	assert_eq!(leaves, expected);

	let monitor = monitor_sd.shared_data();
	assert_eq!(monitor.import_counter, 4);
	assert!(monitor.levels.iter().all(|(number, hashes)| {
		hashes
			.iter()
			.filter(|hash| **hash != block13.header.hash())
			.all(|hash| *number == *monitor.freshness.get(hash).unwrap())
	}));
	assert_eq!(*monitor.freshness.get(&block13.header.hash()).unwrap(), monitor.import_counter);
}

#[test]
fn find_potential_parents_in_allowed_ancestry() {
	sp_tracing::try_init_simple();

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);
	let block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	{
		let included_map = &mut relay_chain.inner.lock().unwrap().relay_chain_hash_to_header;
		included_map.insert(relay_parent, block.header().clone());
	}

	let potential_parents = block_on(find_potential_parents(
		ParentSearchParams {
			relay_parent,
			para_id: ParaId::from(100),
			ancestry_lookback: 0,
			max_depth: 0,
			ignore_alternative_branches: true,
		},
		&*backend,
		&relay_chain,
	))
	.unwrap();
	assert_eq!(potential_parents.len(), 1);
	let parent = &potential_parents[0];

	assert_eq!(parent.hash, block.hash());
	assert_eq!(&parent.header, block.header());
	assert_eq!(parent.depth, 0);
	assert!(parent.aligned_with_pending);

	// New block is not pending or included.
	let block_relay_parent = relay_hash_from_block_num(11);
	let search_relay_parent = relay_hash_from_block_num(13);
	{
		let included_map = &mut relay_chain.inner.lock().unwrap().relay_chain_hash_to_header;
		included_map.insert(search_relay_parent, block.header().clone());
	}
	let block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		Some(block.header().hash()),
		None,
		Some(block_relay_parent),
	);
	let potential_parents = block_on(find_potential_parents(
		ParentSearchParams {
			relay_parent: search_relay_parent,
			para_id: ParaId::from(100),
			ancestry_lookback: 2,
			max_depth: 1,
			ignore_alternative_branches: true,
		},
		&*backend,
		&relay_chain,
	))
	.unwrap();

	assert_eq!(potential_parents.len(), 2);
	let parent = &potential_parents[1];

	assert_eq!(parent.hash, block.hash());
	assert_eq!(&parent.header, block.header());
	assert_eq!(parent.depth, 1);
	assert!(parent.aligned_with_pending);

	// Reduce allowed ancestry.
	let potential_parents = block_on(find_potential_parents(
		ParentSearchParams {
			relay_parent: search_relay_parent,
			para_id: ParaId::from(100),
			ancestry_lookback: 1,
			max_depth: 1,
			ignore_alternative_branches: true,
		},
		&*backend,
		&relay_chain,
	))
	.unwrap();
	assert_eq!(potential_parents.len(), 1);
	let parent = &potential_parents[0];
	assert_ne!(parent.hash, block.hash());
}

/// Tests that pending availability block is always potential parent.
#[test]
fn find_potential_pending_parent() {
	sp_tracing::try_init_simple();

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);
	let included_block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);
	let relay_parent = relay_hash_from_block_num(12);
	let pending_block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		Some(included_block.header().hash()),
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	let search_relay_parent = relay_hash_from_block_num(15);
	{
		let relay_inner = &mut relay_chain.inner.lock().unwrap();
		relay_inner
			.relay_chain_hash_to_header
			.insert(search_relay_parent, included_block.header().clone());
		relay_inner
			.relay_chain_hash_to_header_pending
			.insert(search_relay_parent, pending_block.header().clone());
	}

	let potential_parents = block_on(find_potential_parents(
		ParentSearchParams {
			relay_parent: search_relay_parent,
			para_id: ParaId::from(100),
			ancestry_lookback: 0,
			max_depth: 1,
			ignore_alternative_branches: true,
		},
		&*backend,
		&relay_chain,
	))
	.unwrap();
	assert_eq!(potential_parents.len(), 2);
	let included_parent = &potential_parents[0];

	assert_eq!(included_parent.hash, included_block.hash());
	assert_eq!(&included_parent.header, included_block.header());
	assert_eq!(included_parent.depth, 0);
	assert!(included_parent.aligned_with_pending);

	let pending_parent = &potential_parents[1];

	assert_eq!(pending_parent.hash, pending_block.hash());
	assert_eq!(&pending_parent.header, pending_block.header());
	assert_eq!(pending_parent.depth, 1);
	assert!(pending_parent.aligned_with_pending);
}

#[test]
fn find_potential_parents_with_max_depth() {
	sp_tracing::try_init_simple();

	const NON_INCLUDED_CHAIN_LEN: usize = 5;

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);
	let included_block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	{
		let included_map = &mut relay_chain.inner.lock().unwrap().relay_chain_hash_to_header;
		included_map.insert(relay_parent, included_block.header().clone());
	}

	let mut blocks = Vec::new();
	let mut parent = included_block.header().hash();
	for _ in 0..NON_INCLUDED_CHAIN_LEN {
		let block = build_and_import_block_ext(
			&client,
			BlockOrigin::Own,
			true,
			&mut para_import,
			Some(parent),
			None,
			Some(relay_parent),
		);
		parent = block.header().hash();
		blocks.push(block);
	}
	for max_depth in 0..=NON_INCLUDED_CHAIN_LEN {
		let potential_parents = block_on(find_potential_parents(
			ParentSearchParams {
				relay_parent,
				para_id: ParaId::from(100),
				ancestry_lookback: 0,
				max_depth,
				ignore_alternative_branches: true,
			},
			&*backend,
			&relay_chain,
		))
		.unwrap();
		assert_eq!(potential_parents.len(), max_depth + 1);
		let expected_parents: Vec<_> =
			std::iter::once(&included_block).chain(blocks.iter().take(max_depth)).collect();

		for i in 0..(max_depth + 1) {
			let parent = &potential_parents[i];
			let expected = &expected_parents[i];

			assert_eq!(parent.hash, expected.hash());
			assert_eq!(&parent.header, expected.header());
			assert_eq!(parent.depth, i);
			assert!(parent.aligned_with_pending);
		}
	}
}

#[test]
fn find_potential_parents_unknown_included() {
	sp_tracing::try_init_simple();

	const NON_INCLUDED_CHAIN_LEN: usize = 5;

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let relay_parent = relay_hash_from_block_num(10);
	// Choose different relay parent for alternative chain to get new hashes.
	let search_relay_parent = relay_hash_from_block_num(11);

	let sproof = sproof_with_best_parent(&client);
	let included_but_unknown = build_block(&*client, sproof, None, None, Some(relay_parent));

	let relay_chain = Relaychain::new();
	{
		let relay_inner = &mut relay_chain.inner.lock().unwrap();
		relay_inner
			.relay_chain_hash_to_header
			.insert(search_relay_parent, included_but_unknown.header().clone());
	}

	// Ignore alternative branch:
	let potential_parents = block_on(find_potential_parents(
		ParentSearchParams {
			relay_parent: search_relay_parent,
			para_id: ParaId::from(100),
			ancestry_lookback: 1, // aligned chain is in ancestry.
			max_depth: NON_INCLUDED_CHAIN_LEN,
			ignore_alternative_branches: true,
		},
		&*backend,
		&relay_chain,
	))
	.unwrap();

	assert_eq!(potential_parents.len(), 0);
}

#[test]
fn find_potential_parents_unknown_pending() {
	sp_tracing::try_init_simple();

	const NON_INCLUDED_CHAIN_LEN: usize = 5;

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import =
		ParachainBlockImport::new_with_delayed_best_block(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);
	// Choose different relay parent for alternative chain to get new hashes.
	let search_relay_parent = relay_hash_from_block_num(11);
	let included_block = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);

	let sproof = sproof_with_parent_by_hash(&client, included_block.header().hash());
	let pending_but_unknown = build_block(
		&*client,
		sproof,
		Some(included_block.header().hash()),
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	{
		let relay_inner = &mut relay_chain.inner.lock().unwrap();
		relay_inner
			.relay_chain_hash_to_header
			.insert(search_relay_parent, included_block.header().clone());
		relay_inner
			.relay_chain_hash_to_header_pending
			.insert(search_relay_parent, pending_but_unknown.header().clone());
	}

	// Ignore alternative branch:
	let potential_parents = block_on(find_potential_parents(
		ParentSearchParams {
			relay_parent: search_relay_parent,
			para_id: ParaId::from(100),
			ancestry_lookback: 1, // aligned chain is in ancestry.
			max_depth: NON_INCLUDED_CHAIN_LEN,
			ignore_alternative_branches: true,
		},
		&*backend,
		&relay_chain,
	))
	.unwrap();

	assert!(potential_parents.is_empty());
}

#[test]
fn find_potential_parents_unknown_pending_include_alternative_branches() {
	sp_tracing::try_init_simple();

	const NON_INCLUDED_CHAIN_LEN: usize = 5;

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import =
		ParachainBlockImport::new_with_delayed_best_block(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);

	// Choose different relay parent for alternative chain to get new hashes.
	let search_relay_parent = relay_hash_from_block_num(11);

	let included_block = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);

	let alt_block = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		Some(included_block.header().hash()),
		None,
		Some(search_relay_parent),
	);

	tracing::info!(hash = %alt_block.header().hash(), "Alt block.");
	let sproof = sproof_with_parent_by_hash(&client, included_block.header().hash());
	let pending_but_unknown = build_block(
		&*client,
		sproof,
		Some(included_block.header().hash()),
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	{
		let relay_inner = &mut relay_chain.inner.lock().unwrap();
		relay_inner
			.relay_chain_hash_to_header
			.insert(search_relay_parent, included_block.header().clone());
		relay_inner
			.relay_chain_hash_to_header_pending
			.insert(search_relay_parent, pending_but_unknown.header().clone());
	}

	// Ignore alternative branch:
	let potential_parents = block_on(find_potential_parents(
		ParentSearchParams {
			relay_parent: search_relay_parent,
			para_id: ParaId::from(100),
			ancestry_lookback: 1, // aligned chain is in ancestry.
			max_depth: NON_INCLUDED_CHAIN_LEN,
			ignore_alternative_branches: false,
		},
		&*backend,
		&relay_chain,
	))
	.unwrap();

	let expected_parents: Vec<_> = vec![&included_block, &alt_block];
	assert_eq!(potential_parents.len(), 2);
	assert_eq!(expected_parents[0].hash(), potential_parents[0].hash);
	assert_eq!(expected_parents[1].hash(), potential_parents[1].hash);
}

/// Test where there are multiple pending blocks.
#[test]
fn find_potential_parents_aligned_with_late_pending() {
	sp_tracing::try_init_simple();

	const NON_INCLUDED_CHAIN_LEN: usize = 5;

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import =
		ParachainBlockImport::new_with_delayed_best_block(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);
	// Choose different relay parent for alternative chain to get new hashes.
	let search_relay_parent = relay_hash_from_block_num(11);
	let included_block = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);

	let in_between_block = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		Some(included_block.header().hash()),
		None,
		Some(relay_parent),
	);

	let pending_block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		Some(in_between_block.header().hash()),
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	{
		let relay_inner = &mut relay_chain.inner.lock().unwrap();
		relay_inner
			.relay_chain_hash_to_header
			.insert(search_relay_parent, included_block.header().clone());
		relay_inner
			.relay_chain_hash_to_header_pending
			.insert(search_relay_parent, in_between_block.header().clone());
		relay_inner
			.relay_chain_hash_to_header_pending
			.insert(search_relay_parent, pending_block.header().clone());
	}

	// Build some blocks on the pending block and on the included block.
	// We end up with two sibling chains, one is aligned with the pending block,
	// the other is not.
	let mut aligned_blocks = Vec::new();
	let mut parent = pending_block.header().hash();
	for _ in 2..NON_INCLUDED_CHAIN_LEN {
		let block = build_and_import_block_ext(
			&client,
			BlockOrigin::Own,
			true,
			&mut para_import,
			Some(parent),
			None,
			Some(relay_parent),
		);
		parent = block.header().hash();
		aligned_blocks.push(block);
	}

	let mut alt_blocks = Vec::new();
	let mut parent = included_block.header().hash();
	for _ in 0..NON_INCLUDED_CHAIN_LEN {
		let block = build_and_import_block_ext(
			&client,
			BlockOrigin::NetworkInitialSync,
			true,
			&mut para_import,
			Some(parent),
			None,
			Some(search_relay_parent),
		);
		parent = block.header().hash();
		alt_blocks.push(block);
	}

	// Ignore alternative branch:
	for max_depth in 0..=NON_INCLUDED_CHAIN_LEN {
		let potential_parents = block_on(find_potential_parents(
			ParentSearchParams {
				relay_parent: search_relay_parent,
				para_id: ParaId::from(100),
				ancestry_lookback: 1, // aligned chain is in ancestry.
				max_depth,
				ignore_alternative_branches: true,
			},
			&*backend,
			&relay_chain,
		))
		.unwrap();

		assert_eq!(potential_parents.len(), max_depth + 1);
		let expected_parents: Vec<_> = [&included_block, &in_between_block, &pending_block]
			.into_iter()
			.chain(aligned_blocks.iter())
			.take(max_depth + 1)
			.collect();

		for i in 0..(max_depth + 1) {
			let parent = &potential_parents[i];
			let expected = &expected_parents[i];

			assert_eq!(parent.hash, expected.hash());
			assert_eq!(&parent.header, expected.header());
			assert_eq!(parent.depth, i);
			assert!(parent.aligned_with_pending);
		}
	}

	// Do not ignore:
	for max_depth in 0..=NON_INCLUDED_CHAIN_LEN {
		let potential_parents = block_on(find_potential_parents(
			ParentSearchParams {
				relay_parent: search_relay_parent,
				para_id: ParaId::from(100),
				ancestry_lookback: 1, // aligned chain is in ancestry.
				max_depth,
				ignore_alternative_branches: false,
			},
			&*backend,
			&relay_chain,
		))
		.unwrap();

		let expected_len = 2 * max_depth + 1;
		assert_eq!(potential_parents.len(), expected_len);
		let expected_aligned: Vec<_> = [&included_block, &in_between_block, &pending_block]
			.into_iter()
			.chain(aligned_blocks.iter())
			.take(max_depth + 1)
			.collect();
		let expected_alt = alt_blocks.iter().take(max_depth);

		let expected_parents: Vec<_> =
			expected_aligned.clone().into_iter().chain(expected_alt).collect();
		// Check correctness.
		assert_eq!(expected_parents.len(), expected_len);

		for i in 0..expected_len {
			let parent = &potential_parents[i];
			let expected = expected_parents
				.iter()
				.find(|block| block.header().hash() == parent.hash)
				.expect("missing parent");

			let is_aligned = expected_aligned.contains(&expected);

			assert_eq!(parent.hash, expected.hash());
			assert_eq!(&parent.header, expected.header());

			assert_eq!(parent.aligned_with_pending, is_aligned);
		}
	}
}

#[test]
fn find_potential_parents_aligned_with_pending() {
	sp_tracing::try_init_simple();

	const NON_INCLUDED_CHAIN_LEN: usize = 5;

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import =
		ParachainBlockImport::new_with_delayed_best_block(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);
	// Choose different relay parent for alternative chain to get new hashes.
	let search_relay_parent = relay_hash_from_block_num(11);
	let included_block = build_and_import_block_ext(
		&client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);
	let pending_block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		Some(included_block.header().hash()),
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	{
		let relay_inner = &mut relay_chain.inner.lock().unwrap();
		relay_inner
			.relay_chain_hash_to_header
			.insert(search_relay_parent, included_block.header().clone());
		relay_inner
			.relay_chain_hash_to_header_pending
			.insert(search_relay_parent, pending_block.header().clone());
	}

	// Build two sibling chains from the included block.
	let mut aligned_blocks = Vec::new();
	let mut parent = pending_block.header().hash();
	for _ in 1..NON_INCLUDED_CHAIN_LEN {
		let block = build_and_import_block_ext(
			&client,
			BlockOrigin::Own,
			true,
			&mut para_import,
			Some(parent),
			None,
			Some(relay_parent),
		);
		parent = block.header().hash();
		aligned_blocks.push(block);
	}

	let mut alt_blocks = Vec::new();
	let mut parent = included_block.header().hash();
	for _ in 0..NON_INCLUDED_CHAIN_LEN {
		let block = build_and_import_block_ext(
			&client,
			BlockOrigin::NetworkInitialSync,
			true,
			&mut para_import,
			Some(parent),
			None,
			Some(search_relay_parent),
		);
		parent = block.header().hash();
		alt_blocks.push(block);
	}

	// Ignore alternative branch:
	for max_depth in 0..=NON_INCLUDED_CHAIN_LEN {
		let potential_parents = block_on(find_potential_parents(
			ParentSearchParams {
				relay_parent: search_relay_parent,
				para_id: ParaId::from(100),
				ancestry_lookback: 1, // aligned chain is in ancestry.
				max_depth,
				ignore_alternative_branches: true,
			},
			&*backend,
			&relay_chain,
		))
		.unwrap();
		assert_eq!(potential_parents.len(), max_depth + 1);
		let expected_parents: Vec<_> = [&included_block, &pending_block]
			.into_iter()
			.chain(aligned_blocks.iter())
			.take(max_depth + 1)
			.collect();

		for i in 0..(max_depth + 1) {
			let parent = &potential_parents[i];
			let expected = &expected_parents[i];

			assert_eq!(parent.hash, expected.hash());
			assert_eq!(&parent.header, expected.header());
			assert_eq!(parent.depth, i);
			assert!(parent.aligned_with_pending);
		}
	}

	// Do not ignore:
	for max_depth in 0..=NON_INCLUDED_CHAIN_LEN {
		log::info!("Ran with max_depth = {max_depth}");
		let potential_parents = block_on(find_potential_parents(
			ParentSearchParams {
				relay_parent: search_relay_parent,
				para_id: ParaId::from(100),
				ancestry_lookback: 1, // aligned chain is in ancestry.
				max_depth,
				ignore_alternative_branches: false,
			},
			&*backend,
			&relay_chain,
		))
		.unwrap();

		let expected_len = 2 * max_depth + 1;
		assert_eq!(potential_parents.len(), expected_len);
		let expected_aligned: Vec<_> = [&included_block, &pending_block]
			.into_iter()
			.chain(aligned_blocks.iter())
			.take(max_depth + 1)
			.collect();
		let expected_alt = alt_blocks.iter().take(max_depth);

		let expected_parents: Vec<_> =
			expected_aligned.clone().into_iter().chain(expected_alt).collect();
		// Check correctness.
		assert_eq!(expected_parents.len(), expected_len);

		potential_parents.iter().for_each(|p| log::info!("result: {:?}", p));
		for i in 0..expected_len {
			let parent = &potential_parents[i];
			let expected = expected_parents
				.iter()
				.find(|block| block.header().hash() == parent.hash)
				.expect("missing parent");

			let is_aligned = expected_aligned.contains(&expected);

			assert_eq!(parent.hash, expected.hash());
			assert_eq!(&parent.header, expected.header());

			log::info!(
				"Check hash: {:?} expected: {} is: {}",
				parent.hash,
				is_aligned,
				parent.aligned_with_pending,
			);
			assert_eq!(parent.aligned_with_pending, is_aligned);
		}
	}
}

/// Tests that no potential parent gets discarded if there's no pending availability block.
#[test]
fn find_potential_parents_aligned_no_pending() {
	sp_tracing::try_init_simple();

	const NON_INCLUDED_CHAIN_LEN: usize = 5;

	let backend = Arc::new(Backend::new_test(1000, 1));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import =
		ParachainBlockImport::new_with_delayed_best_block(client.clone(), backend.clone());

	let relay_parent = relay_hash_from_block_num(10);
	// Choose different relay parent for alternative chain to get new hashes.
	let search_relay_parent = relay_hash_from_block_num(11);
	let included_block = build_and_import_block_ext(
		&client,
		BlockOrigin::Own,
		true,
		&mut para_import,
		None,
		None,
		Some(relay_parent),
	);

	let relay_chain = Relaychain::new();
	{
		let included_map = &mut relay_chain.inner.lock().unwrap().relay_chain_hash_to_header;
		included_map.insert(search_relay_parent, included_block.header().clone());
	}

	// Build two sibling chains from the included block.
	let mut parent = included_block.header().hash();
	for _ in 0..NON_INCLUDED_CHAIN_LEN {
		let block = build_and_import_block_ext(
			&client,
			BlockOrigin::Own,
			true,
			&mut para_import,
			Some(parent),
			None,
			Some(relay_parent),
		);
		parent = block.header().hash();
	}

	let mut parent = included_block.header().hash();
	for _ in 0..NON_INCLUDED_CHAIN_LEN {
		let block = build_and_import_block_ext(
			&client,
			BlockOrigin::NetworkInitialSync,
			true,
			&mut para_import,
			Some(parent),
			None,
			Some(search_relay_parent),
		);
		parent = block.header().hash();
	}

	for max_depth in 0..=NON_INCLUDED_CHAIN_LEN {
		let potential_parents_aligned = block_on(find_potential_parents(
			ParentSearchParams {
				relay_parent: search_relay_parent,
				para_id: ParaId::from(100),
				ancestry_lookback: 1, // aligned chain is in ancestry.
				max_depth,
				ignore_alternative_branches: true,
			},
			&*backend,
			&relay_chain,
		))
		.unwrap();
		let potential_parents = block_on(find_potential_parents(
			ParentSearchParams {
				relay_parent: search_relay_parent,
				para_id: ParaId::from(100),
				ancestry_lookback: 1,
				max_depth,
				ignore_alternative_branches: false,
			},
			&*backend,
			&relay_chain,
		))
		.unwrap();
		assert_eq!(potential_parents.len(), 2 * max_depth + 1);
		assert_eq!(potential_parents, potential_parents_aligned);
	}
}
