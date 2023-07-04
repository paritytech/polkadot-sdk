// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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
use cumulus_primitives_core::{relay_chain::BlockId, InboundDownwardMessage, InboundHrmpMessage};
use cumulus_relay_chain_interface::{
	CommittedCandidateReceipt, OccupiedCoreAssumption, OverseerHandle, PHeader, ParaId,
	RelayChainInterface, RelayChainResult, SessionIndex, StorageValue, ValidatorId,
};
use cumulus_test_client::{
	runtime::{Block, Hash, Header},
	Backend, Client, InitBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use futures::{channel::mpsc, executor::block_on, select, FutureExt, Stream, StreamExt};
use futures_timer::Delay;
use sc_client_api::{blockchain::Backend as _, Backend as _, UsageProvider};
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy};
use sp_consensus::{BlockOrigin, BlockStatus};
use std::{
	collections::{BTreeMap, HashMap},
	pin::Pin,
	sync::{Arc, Mutex},
	time::Duration,
};

struct RelaychainInner {
	new_best_heads: Option<mpsc::UnboundedReceiver<Header>>,
	finalized_heads: Option<mpsc::UnboundedReceiver<Header>>,
	new_best_heads_sender: mpsc::UnboundedSender<Header>,
	finalized_heads_sender: mpsc::UnboundedSender<Header>,
	relay_chain_hash_to_header: HashMap<PHash, Header>,
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
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		Ok(Some(PersistedValidationData {
			parent_head: self
				.inner
				.lock()
				.unwrap()
				.relay_chain_hash_to_header
				.get(&hash)
				.unwrap()
				.encode()
				.into(),
			..Default::default()
		}))
	}

	async fn candidate_pending_availability(
		&self,
		_: PHash,
		_: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		unimplemented!("Not needed for test")
	}

	async fn session_index_for_child(&self, _: PHash) -> RelayChainResult<SessionIndex> {
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
		unimplemented!("Not needed for test")
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

	async fn header(&self, _block_id: BlockId) -> RelayChainResult<Option<PHeader>> {
		unimplemented!("Not needed for test")
	}
}

fn build_block<B: InitBlockBuilder>(
	builder: &B,
	at: Option<Hash>,
	timestamp: Option<u64>,
) -> Block {
	let builder = match at {
		Some(at) => match timestamp {
			Some(ts) => builder.init_block_builder_with_timestamp(at, None, Default::default(), ts),
			None => builder.init_block_builder_at(at, None, Default::default()),
		},
		None => builder.init_block_builder(None, Default::default()),
	};

	let mut block = builder.build().unwrap().block;

	// Simulate some form of post activity (like a Seal or Other generic things).
	// This is mostly used to exercise the `LevelMonitor` correct behavior.
	// (in practice we want that header post-hash != pre-hash)
	block.header.digest.push(sp_runtime::DigestItem::Other(vec![1, 2, 3]));

	block
}

async fn import_block<I: BlockImport<Block>>(
	importer: &mut I,
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

fn build_and_import_block_ext<B: InitBlockBuilder, I: BlockImport<Block>>(
	builder: &B,
	origin: BlockOrigin,
	import_as_best: bool,
	importer: &mut I,
	at: Option<Hash>,
	timestamp: Option<u64>,
) -> Block {
	let block = build_block(builder, at, timestamp);
	import_block_sync(importer, block.clone(), origin, import_as_best);
	block
}

fn build_and_import_block(mut client: Arc<Client>, import_as_best: bool) -> Block {
	build_and_import_block_ext(
		&*client.clone(),
		BlockOrigin::Own,
		import_as_best,
		&mut client,
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

	let block = build_block(&*client, None, None);
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
		let block_builder = client.init_block_builder_at(block.hash(), None, Default::default());
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

	let mut client = Arc::new(TestClientBuilder::default().build());

	let block = build_and_import_block(client.clone(), false);

	let unknown_block = {
		let block_builder = client.init_block_builder_at(block.hash(), None, Default::default());
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

		// Now import the unkown block to make it "known"
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
	const TIMESTAMP_MULTIPLIER: u64 = 60000;

	let backend = Arc::new(Backend::new_test(1000, 3));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());
	let mut para_import = ParachainBlockImport::new_with_limit(
		client.clone(),
		backend.clone(),
		LevelLimit::Some(LEVEL_LIMIT),
	);

	let block0 = build_and_import_block_ext(
		&*client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		None,
		None,
	);
	let id0 = block0.header.hash();

	let blocks1 = (0..LEVEL_LIMIT)
		.map(|i| {
			build_and_import_block_ext(
				&*client,
				if i == 1 { BlockOrigin::NetworkInitialSync } else { BlockOrigin::Own },
				i == 1,
				&mut para_import,
				Some(id0),
				Some(i as u64 * TIMESTAMP_MULTIPLIER),
			)
		})
		.collect::<Vec<_>>();
	let id10 = blocks1[0].header.hash();

	let blocks2 = (0..2)
		.map(|i| {
			build_and_import_block_ext(
				&*client,
				BlockOrigin::Own,
				false,
				&mut para_import,
				Some(id10),
				Some(i as u64 * TIMESTAMP_MULTIPLIER),
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
		&*client,
		BlockOrigin::Own,
		false,
		&mut para_import,
		Some(id0),
		Some(LEVEL_LIMIT as u64 * TIMESTAMP_MULTIPLIER),
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
		&*client,
		BlockOrigin::Own,
		false,
		&mut para_import,
		Some(id0),
		Some(2 * LEVEL_LIMIT as u64 * TIMESTAMP_MULTIPLIER),
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
	const TIMESTAMP_MULTIPLIER: u64 = 60000;

	let backend = Arc::new(Backend::new_test(1000, 3));
	let client = Arc::new(TestClientBuilder::with_backend(backend.clone()).build());

	// Start with a block import not enforcing any limit...
	let mut para_import = ParachainBlockImport::new_with_limit(
		client.clone(),
		backend.clone(),
		LevelLimit::Some(usize::MAX),
	);

	let block00 = build_and_import_block_ext(
		&*client,
		BlockOrigin::NetworkInitialSync,
		true,
		&mut para_import,
		None,
		None,
	);
	let id00 = block00.header.hash();

	let blocks1 = (0..LEVEL_LIMIT + 1)
		.map(|i| {
			build_and_import_block_ext(
				&*client,
				if i == 1 { BlockOrigin::NetworkInitialSync } else { BlockOrigin::Own },
				i == 1,
				&mut para_import,
				Some(id00),
				Some(i as u64 * TIMESTAMP_MULTIPLIER),
			)
		})
		.collect::<Vec<_>>();
	let id10 = blocks1[0].header.hash();

	let _ = (0..LEVEL_LIMIT)
		.map(|i| {
			build_and_import_block_ext(
				&*client,
				BlockOrigin::Own,
				false,
				&mut para_import,
				Some(id10),
				Some(i as u64 * TIMESTAMP_MULTIPLIER),
			)
		})
		.collect::<Vec<_>>();

	// Scenario before limit application (with B11 imported as best)
	// Import order (freshess): B00, B10, B11, B12, B20, B21
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
		&*client,
		BlockOrigin::Own,
		false,
		&mut para_import,
		Some(id00),
		Some(LEVEL_LIMIT as u64 * TIMESTAMP_MULTIPLIER),
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
