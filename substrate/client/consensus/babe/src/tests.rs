// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! BABE testsuite

// FIXME #2532: need to allow deprecated until refactor is done
// https://github.com/paritytech/substrate/issues/2532
#![allow(deprecated)]
use super::*;
use authorship::claim_slot;

use sp_consensus_babe::{AuthorityPair, SlotNumber};
use sc_block_builder::{BlockBuilder, BlockBuilderProvider};
use sp_consensus::{
	NoNetwork as DummyOracle, Proposal, RecordProof,
	import_queue::{BoxBlockImport, BoxJustificationImport, BoxFinalityProofImport},
};
use sc_network_test::*;
use sc_network_test::{Block as TestBlock, PeersClient};
use sc_network::config::{BoxFinalityProofRequestBuilder, ProtocolConfig};
use sp_runtime::{generic::DigestItem, traits::{Block as BlockT, DigestFor}};
use sc_client_api::{BlockchainEvents, backend::TransactionFor};
use log::debug;
use std::{time::Duration, cell::RefCell, task::Poll};

type Item = DigestItem<Hash>;

type Error = sp_blockchain::Error;

type TestClient = sc_client::Client<
	substrate_test_runtime_client::Backend,
	substrate_test_runtime_client::Executor,
	TestBlock,
	substrate_test_runtime_client::runtime::RuntimeApi,
>;

#[derive(Copy, Clone, PartialEq)]
enum Stage {
	PreSeal,
	PostSeal,
}

type Mutator = Arc<dyn Fn(&mut TestHeader, Stage) + Send + Sync>;

#[derive(Clone)]
struct DummyFactory {
	client: Arc<TestClient>,
	epoch_changes: SharedEpochChanges<TestBlock, Epoch>,
	config: Config,
	mutator: Mutator,
}

struct DummyProposer {
	factory: DummyFactory,
	parent_hash: Hash,
	parent_number: u64,
	parent_slot: SlotNumber,
}

impl Environment<TestBlock> for DummyFactory {
	type CreateProposer = future::Ready<Result<DummyProposer, Error>>;
	type Proposer = DummyProposer;
	type Error = Error;

	fn init(&mut self, parent_header: &<TestBlock as BlockT>::Header)
		-> Self::CreateProposer
	{

		let parent_slot = crate::find_pre_digest::<TestBlock>(parent_header)
			.expect("parent header has a pre-digest")
			.slot_number();

		future::ready(Ok(DummyProposer {
			factory: self.clone(),
			parent_hash: parent_header.hash(),
			parent_number: *parent_header.number(),
			parent_slot,
		}))
	}
}

impl DummyProposer {
	fn propose_with(&mut self, pre_digests: DigestFor<TestBlock>)
		-> future::Ready<
			Result<
				Proposal<
					TestBlock,
					sc_client_api::TransactionFor<substrate_test_runtime_client::Backend, TestBlock>
				>,
				Error
			>
		>
	{
		let block_builder = self.factory.client.new_block_at(
			&BlockId::Hash(self.parent_hash),
			pre_digests,
			false,
		).unwrap();

		let mut block = match block_builder.build().map_err(|e| e.into()) {
			Ok(b) => b.block,
			Err(e) => return future::ready(Err(e)),
		};

		let this_slot = crate::find_pre_digest::<TestBlock>(block.header())
			.expect("baked block has valid pre-digest")
			.slot_number();

		// figure out if we should add a consensus digest, since the test runtime
		// doesn't.
		let epoch_changes = self.factory.epoch_changes.lock();
		let epoch = epoch_changes.epoch_for_child_of(
			descendent_query(&*self.factory.client),
			&self.parent_hash,
			self.parent_number,
			this_slot,
			|slot| self.factory.config.genesis_epoch(slot),
		)
			.expect("client has data to find epoch")
			.expect("can compute epoch for baked block")
			.into_inner();

		let first_in_epoch = self.parent_slot < epoch.start_slot;
		if first_in_epoch {
			// push a `Consensus` digest signalling next change.
			// we just reuse the same randomness and authorities as the prior
			// epoch. this will break when we add light client support, since
			// that will re-check the randomness logic off-chain.
			let digest_data = ConsensusLog::NextEpochData(NextEpochDescriptor {
				authorities: epoch.authorities.clone(),
				randomness: epoch.randomness.clone(),
			}).encode();
			let digest = DigestItem::Consensus(BABE_ENGINE_ID, digest_data);
			block.header.digest_mut().push(digest)
		}

		// mutate the block header according to the mutator.
		(self.factory.mutator)(&mut block.header, Stage::PreSeal);

		future::ready(Ok(Proposal { block, proof: None, storage_changes: Default::default() }))
	}
}

impl Proposer<TestBlock> for DummyProposer {
	type Error = Error;
	type Transaction = sc_client_api::TransactionFor<substrate_test_runtime_client::Backend, TestBlock>;
	type Proposal = future::Ready<Result<Proposal<TestBlock, Self::Transaction>, Error>>;

	fn propose(
		&mut self,
		_: InherentData,
		pre_digests: DigestFor<TestBlock>,
		_: Duration,
		_: RecordProof,
	) -> Self::Proposal {
		self.propose_with(pre_digests)
	}
}

thread_local! {
	static MUTATOR: RefCell<Mutator> = RefCell::new(Arc::new(|_, _|()));
}

#[derive(Clone)]
struct PanickingBlockImport<B>(B);

impl<B: BlockImport<TestBlock>> BlockImport<TestBlock> for PanickingBlockImport<B> {
	type Error = B::Error;
	type Transaction = B::Transaction;

	fn import_block(
		&mut self,
		block: BlockImportParams<TestBlock, Self::Transaction>,
		new_cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		Ok(self.0.import_block(block, new_cache).expect("importing block failed"))
	}

	fn check_block(
		&mut self,
		block: BlockCheckParams<TestBlock>,
	) -> Result<ImportResult, Self::Error> {
		Ok(self.0.check_block(block).expect("checking block failed"))
	}
}

pub struct BabeTestNet {
	peers: Vec<Peer<Option<PeerData>>>,
}

type TestHeader = <TestBlock as BlockT>::Header;
type TestExtrinsic = <TestBlock as BlockT>::Extrinsic;

pub struct TestVerifier {
	inner: BabeVerifier<TestBlock, PeersFullClient>,
	mutator: Mutator,
}

impl Verifier<TestBlock> for TestVerifier {
	/// Verify the given data and return the BlockImportParams and an optional
	/// new set of validators to import. If not, err with an Error-Message
	/// presented to the User in the logs.
	fn verify(
		&mut self,
		origin: BlockOrigin,
		mut header: TestHeader,
		justification: Option<Justification>,
		body: Option<Vec<TestExtrinsic>>,
	) -> Result<(BlockImportParams<TestBlock, ()>, Option<Vec<(CacheKeyId, Vec<u8>)>>), String> {
		// apply post-sealing mutations (i.e. stripping seal, if desired).
		(self.mutator)(&mut header, Stage::PostSeal);
		self.inner.verify(origin, header, justification, body)
	}
}

pub struct PeerData {
	link: BabeLink<TestBlock>,
	inherent_data_providers: InherentDataProviders,
	block_import: Mutex<
		Option<BoxBlockImport<TestBlock, TransactionFor<substrate_test_runtime_client::Backend, TestBlock>>>
	>,
}

impl TestNetFactory for BabeTestNet {
	type Verifier = TestVerifier;
	type PeerData = Option<PeerData>;

	/// Create new test network with peers and given config.
	fn from_config(_config: &ProtocolConfig) -> Self {
		debug!(target: "babe", "Creating test network from config");
		BabeTestNet {
			peers: Vec::new(),
		}
	}

	fn make_block_import<Transaction>(&self, client: PeersClient)
		-> (
			BlockImportAdapter<Transaction>,
			Option<BoxJustificationImport<Block>>,
			Option<BoxFinalityProofImport<Block>>,
			Option<BoxFinalityProofRequestBuilder<Block>>,
			Option<PeerData>,
		)
	{
		let client = client.as_full().expect("only full clients are tested");
		let inherent_data_providers = InherentDataProviders::new();

		let config = Config::get_or_compute(&*client).expect("config available");
		let (block_import, link) = crate::block_import(
			config,
			client.clone(),
			client.clone(),
		).expect("can initialize block-import");

		let block_import = PanickingBlockImport(block_import);

		let data_block_import = Mutex::new(
			Some(Box::new(block_import.clone()) as BoxBlockImport<_, _>)
		);
		(
			BlockImportAdapter::new_full(block_import),
			None,
			None,
			None,
			Some(PeerData { link, inherent_data_providers, block_import: data_block_import }),
		)
	}

	fn make_verifier(
		&self,
		client: PeersClient,
		_cfg: &ProtocolConfig,
		maybe_link: &Option<PeerData>,
	)
		-> Self::Verifier
	{
		let client = client.as_full().expect("only full clients are used in test");
		trace!(target: "babe", "Creating a verifier");

		// ensure block import and verifier are linked correctly.
		let data = maybe_link.as_ref().expect("babe link always provided to verifier instantiation");

		TestVerifier {
			inner: BabeVerifier {
				client: client.clone(),
				inherent_data_providers: data.inherent_data_providers.clone(),
				config: data.link.config.clone(),
				epoch_changes: data.link.epoch_changes.clone(),
				time_source: data.link.time_source.clone(),
			},
			mutator: MUTATOR.with(|m| m.borrow().clone()),
		}
	}

	fn peer(&mut self, i: usize) -> &mut Peer<Self::PeerData> {
		trace!(target: "babe", "Retrieving a peer");
		&mut self.peers[i]
	}

	fn peers(&self) -> &Vec<Peer<Self::PeerData>> {
		trace!(target: "babe", "Retrieving peers");
		&self.peers
	}

	fn mut_peers<F: FnOnce(&mut Vec<Peer<Self::PeerData>>)>(
		&mut self,
		closure: F,
	) {
		closure(&mut self.peers);
	}
}

#[test]
#[should_panic]
fn rejects_empty_block() {
	env_logger::try_init().unwrap();
	let mut net = BabeTestNet::new(3);
	let block_builder = |builder: BlockBuilder<_, _, _>| {
		builder.build().unwrap().block
	};
	net.mut_peers(|peer| {
		peer[0].generate_blocks(1, BlockOrigin::NetworkInitialSync, block_builder);
	})
}

fn run_one_test(
	mutator: impl Fn(&mut TestHeader, Stage) + Send + Sync + 'static,
) {
	let _ = env_logger::try_init();
	let mutator = Arc::new(mutator) as Mutator;

	MUTATOR.with(|m| *m.borrow_mut() = mutator.clone());
	let net = BabeTestNet::new(3);

	let peers = &[
		(0, "//Alice"),
		(1, "//Bob"),
		(2, "//Charlie"),
	];

	let net = Arc::new(Mutex::new(net));
	let mut import_notifications = Vec::new();
	let mut babe_futures = Vec::new();
	let mut keystore_paths = Vec::new();

	for (peer_id, seed) in peers {
		let mut net = net.lock();
		let peer = net.peer(*peer_id);
		let client = peer.client().as_full().expect("Only full clients are used in tests").clone();
		let select_chain = peer.select_chain().expect("Full client has select_chain");

		let keystore_path = tempfile::tempdir().expect("Creates keystore path");
		let keystore = sc_keystore::Store::open(keystore_path.path(), None).expect("Creates keystore");
		keystore.write().insert_ephemeral_from_seed::<AuthorityPair>(seed).expect("Generates authority key");
		keystore_paths.push(keystore_path);

		let mut got_own = false;
		let mut got_other = false;

		let data = peer.data.as_ref().expect("babe link set up during initialization");

		let environ = DummyFactory {
			client: client.clone(),
			config: data.link.config.clone(),
			epoch_changes: data.link.epoch_changes.clone(),
			mutator: mutator.clone(),
		};

		import_notifications.push(
			// run each future until we get one of our own blocks with number higher than 5
			// that was produced locally.
			client.import_notification_stream()
				.take_while(move |n| future::ready(n.header.number() < &5 || {
					if n.origin == BlockOrigin::Own {
						got_own = true;
					} else {
						got_other = true;
					}

					// continue until we have at least one block of our own
					// and one of another peer.
					!(got_own && got_other)
				}))
				.for_each(|_| future::ready(()) )
		);


		babe_futures.push(start_babe(BabeParams {
			block_import: data.block_import.lock().take().expect("import set up during init"),
			select_chain,
			client,
			env: environ,
			sync_oracle: DummyOracle,
			inherent_data_providers: data.inherent_data_providers.clone(),
			force_authoring: false,
			babe_link: data.link.clone(),
			keystore,
			can_author_with: sp_consensus::AlwaysCanAuthor,
		}).expect("Starts babe"));
	}

	futures::executor::block_on(future::select(
		futures::future::poll_fn(move |cx| {
			let mut net = net.lock();
			net.poll(cx);
			for p in net.peers() {
				for (h, e) in p.failed_verifications() {
					panic!("Verification failed for {:?}: {}", h, e);
				}
			}
	
			Poll::<()>::Pending
		}),
		future::select(future::join_all(import_notifications), future::join_all(babe_futures))
	));
}

#[test]
fn authoring_blocks() {
	run_one_test(|_, _| ())
}

#[test]
#[should_panic]
fn rejects_missing_inherent_digest() {
	run_one_test(|header: &mut TestHeader, stage| {
		let v = std::mem::replace(&mut header.digest_mut().logs, vec![]);
		header.digest_mut().logs = v.into_iter()
			.filter(|v| stage == Stage::PostSeal || v.as_babe_pre_digest().is_none())
			.collect()
	})
}

#[test]
#[should_panic]
fn rejects_missing_seals() {
	run_one_test(|header: &mut TestHeader, stage| {
		let v = std::mem::replace(&mut header.digest_mut().logs, vec![]);
		header.digest_mut().logs = v.into_iter()
			.filter(|v| stage == Stage::PreSeal || v.as_babe_seal().is_none())
			.collect()
	})
}

#[test]
#[should_panic]
fn rejects_missing_consensus_digests() {
	run_one_test(|header: &mut TestHeader, stage| {
		let v = std::mem::replace(&mut header.digest_mut().logs, vec![]);
		header.digest_mut().logs = v.into_iter()
			.filter(|v| stage == Stage::PostSeal || v.as_next_epoch_descriptor().is_none())
			.collect()
	});
}

#[test]
fn wrong_consensus_engine_id_rejected() {
	let _ = env_logger::try_init();
	let sig = AuthorityPair::generate().0.sign(b"");
	let bad_seal: Item = DigestItem::Seal([0; 4], sig.to_vec());
	assert!(bad_seal.as_babe_pre_digest().is_none());
	assert!(bad_seal.as_babe_seal().is_none())
}

#[test]
fn malformed_pre_digest_rejected() {
	let _ = env_logger::try_init();
	let bad_seal: Item = DigestItem::Seal(BABE_ENGINE_ID, [0; 64].to_vec());
	assert!(bad_seal.as_babe_pre_digest().is_none());
}

#[test]
fn sig_is_not_pre_digest() {
	let _ = env_logger::try_init();
	let sig = AuthorityPair::generate().0.sign(b"");
	let bad_seal: Item = DigestItem::Seal(BABE_ENGINE_ID, sig.to_vec());
	assert!(bad_seal.as_babe_pre_digest().is_none());
	assert!(bad_seal.as_babe_seal().is_some())
}

#[test]
fn can_author_block() {
	let _ = env_logger::try_init();
	let keystore_path = tempfile::tempdir().expect("Creates keystore path");
	let keystore = sc_keystore::Store::open(keystore_path.path(), None).expect("Creates keystore");
	let pair = keystore.write().insert_ephemeral_from_seed::<AuthorityPair>("//Alice")
		.expect("Generates authority pair");

	let mut i = 0;
	let epoch = Epoch {
		start_slot: 0,
		authorities: vec![(pair.public(), 1)],
		randomness: [0; 32],
		epoch_index: 1,
		duration: 100,
	};

	let mut config = crate::BabeConfiguration {
		slot_duration: 1000,
		epoch_length: 100,
		c: (3, 10),
		genesis_authorities: Vec::new(),
		randomness: [0; 32],
		secondary_slots: true,
	};

	// with secondary slots enabled it should never be empty
	match claim_slot(i, &epoch, &config, &keystore) {
		None => i += 1,
		Some(s) => debug!(target: "babe", "Authored block {:?}", s.0),
	}

	// otherwise with only vrf-based primary slots we might need to try a couple
	// of times.
	config.secondary_slots = false;
	loop {
		match claim_slot(i, &epoch, &config, &keystore) {
			None => i += 1,
			Some(s) => {
				debug!(target: "babe", "Authored block {:?}", s.0);
				break;
			}
		}
	}
}

// Propose and import a new BABE block on top of the given parent.
fn propose_and_import_block<Transaction>(
	parent: &TestHeader,
	slot_number: Option<SlotNumber>,
	proposer_factory: &mut DummyFactory,
	block_import: &mut BoxBlockImport<TestBlock, Transaction>,
) -> sp_core::H256 {
	let mut proposer = futures::executor::block_on(proposer_factory.init(parent)).unwrap();

	let slot_number = slot_number.unwrap_or_else(|| {
		let parent_pre_digest = find_pre_digest::<TestBlock>(parent).unwrap();
		parent_pre_digest.slot_number() + 1
	});

	let pre_digest = sp_runtime::generic::Digest {
		logs: vec![
			Item::babe_pre_digest(
				PreDigest::Secondary {
					authority_index: 0,
					slot_number,
				},
			),
		],
	};

	let parent_hash = parent.hash();

	let mut block = futures::executor::block_on(proposer.propose_with(pre_digest)).unwrap().block;

	let epoch = proposer_factory.epoch_changes.lock().epoch_for_child_of(
		descendent_query(&*proposer_factory.client),
		&parent_hash,
		*parent.number(),
		slot_number,
		|slot| proposer_factory.config.genesis_epoch(slot)
	).unwrap().unwrap();

	let seal = {
		// sign the pre-sealed hash of the block and then
		// add it to a digest item.
		let pair = AuthorityPair::from_seed(&[1; 32]);
		let pre_hash = block.header.hash();
		let signature = pair.sign(pre_hash.as_ref());
		Item::babe_seal(signature)
	};

	let post_hash = {
		block.header.digest_mut().push(seal.clone());
		let h = block.header.hash();
		block.header.digest_mut().pop();
		h
	};

	let mut import = BlockImportParams::new(BlockOrigin::Own, block.header);
	import.post_digests.push(seal);
	import.body = Some(block.extrinsics);
	import.intermediates.insert(
		Cow::from(INTERMEDIATE_KEY),
		Box::new(BabeIntermediate { epoch }) as Box<dyn Any>,
	);
	import.fork_choice = Some(ForkChoiceStrategy::LongestChain);
	let import_result = block_import.import_block(import, Default::default()).unwrap();

	match import_result {
		ImportResult::Imported(_) => {},
		_ => panic!("expected block to be imported"),
	}

	post_hash
}

#[test]
fn importing_block_one_sets_genesis_epoch() {
	let mut net = BabeTestNet::new(1);

	let peer = net.peer(0);
	let data = peer.data.as_ref().expect("babe link set up during initialization");
	let client = peer.client().as_full().expect("Only full clients are used in tests").clone();

	let mut proposer_factory = DummyFactory {
		client: client.clone(),
		config: data.link.config.clone(),
		epoch_changes: data.link.epoch_changes.clone(),
		mutator: Arc::new(|_, _| ()),
	};

	let mut block_import = data.block_import.lock().take().expect("import set up during init");

	let genesis_header = client.header(&BlockId::Number(0)).unwrap().unwrap();

	let block_hash = propose_and_import_block(
		&genesis_header,
		Some(999),
		&mut proposer_factory,
		&mut block_import,
	);

	let genesis_epoch = data.link.config.genesis_epoch(999);

	let epoch_changes = data.link.epoch_changes.lock();
	let epoch_for_second_block = epoch_changes.epoch_for_child_of(
		descendent_query(&*client),
		&block_hash,
		1,
		1000,
		|slot| data.link.config.genesis_epoch(slot),
	).unwrap().unwrap().into_inner();

	assert_eq!(epoch_for_second_block, genesis_epoch);
}

#[test]
fn importing_epoch_change_block_prunes_tree() {
	use sc_client_api::Finalizer;

	let mut net = BabeTestNet::new(1);

	let peer = net.peer(0);
	let data = peer.data.as_ref().expect("babe link set up during initialization");

	let client = peer.client().as_full().expect("Only full clients are used in tests").clone();
	let mut block_import = data.block_import.lock().take().expect("import set up during init");
	let epoch_changes = data.link.epoch_changes.clone();

	let mut proposer_factory = DummyFactory {
		client: client.clone(),
		config: data.link.config.clone(),
		epoch_changes: data.link.epoch_changes.clone(),
		mutator: Arc::new(|_, _| ()),
	};

	// This is just boilerplate code for proposing and importing n valid BABE
	// blocks that are built on top of the given parent. The proposer takes care
	// of producing epoch change digests according to the epoch duration (which
	// is set to 6 slots in the test runtime).
	let mut propose_and_import_blocks = |parent_id, n| {
		let mut hashes = Vec::new();
		let mut parent_header = client.header(&parent_id).unwrap().unwrap();

		for _ in 0..n {
			let block_hash = propose_and_import_block(
				&parent_header,
				None,
				&mut proposer_factory,
				&mut block_import,
			);
			hashes.push(block_hash);
			parent_header = client.header(&BlockId::Hash(block_hash)).unwrap().unwrap();
		}

		hashes
	};

	// This is the block tree that we're going to use in this test. Each node
	// represents an epoch change block, the epoch duration is 6 slots.
	//
	//    *---- F (#7)
	//   /                 *------ G (#19) - H (#25)
	//  /                 /
	// A (#1) - B (#7) - C (#13) - D (#19) - E (#25)
	//                              \
	//                               *------ I (#25)

	// Create and import the canon chain and keep track of fork blocks (A, C, D)
	// from the diagram above.
	let canon_hashes = propose_and_import_blocks(BlockId::Number(0), 30);

	// Create the forks
	let fork_1 = propose_and_import_blocks(BlockId::Hash(canon_hashes[0]), 10);
	let fork_2 = propose_and_import_blocks(BlockId::Hash(canon_hashes[12]), 15);
	let fork_3 = propose_and_import_blocks(BlockId::Hash(canon_hashes[18]), 10);

	// We should be tracking a total of 9 epochs in the fork tree
	assert_eq!(
		epoch_changes.lock().tree().iter().count(),
		9,
	);

	// And only one root
	assert_eq!(
		epoch_changes.lock().tree().roots().count(),
		1,
	);

	// We finalize block #13 from the canon chain, so on the next epoch
	// change the tree should be pruned, to not contain F (#7).
	client.finalize_block(BlockId::Hash(canon_hashes[12]), None, false).unwrap();
	propose_and_import_blocks(BlockId::Hash(client.chain_info().best_hash), 7);

	// at this point no hashes from the first fork must exist on the tree
	assert!(
		!epoch_changes.lock().tree().iter().map(|(h, _, _)| h).any(|h| fork_1.contains(h)),
	);

	// but the epoch changes from the other forks must still exist
	assert!(
		epoch_changes.lock().tree().iter().map(|(h, _, _)| h).any(|h| fork_2.contains(h))
	);

	assert!(
		epoch_changes.lock().tree().iter().map(|(h, _, _)| h).any(|h| fork_3.contains(h)),
	);

	// finalizing block #25 from the canon chain should prune out the second fork
	client.finalize_block(BlockId::Hash(canon_hashes[24]), None, false).unwrap();
	propose_and_import_blocks(BlockId::Hash(client.chain_info().best_hash), 8);

	// at this point no hashes from the second fork must exist on the tree
	assert!(
		!epoch_changes.lock().tree().iter().map(|(h, _, _)| h).any(|h| fork_2.contains(h)),
	);

	// while epoch changes from the last fork should still exist
	assert!(
		epoch_changes.lock().tree().iter().map(|(h, _, _)| h).any(|h| fork_3.contains(h)),
	);
}

#[test]
#[should_panic]
fn verify_slots_are_strictly_increasing() {
	let mut net = BabeTestNet::new(1);

	let peer = net.peer(0);
	let data = peer.data.as_ref().expect("babe link set up during initialization");

	let client = peer.client().as_full().expect("Only full clients are used in tests").clone();
	let mut block_import = data.block_import.lock().take().expect("import set up during init");

	let mut proposer_factory = DummyFactory {
		client: client.clone(),
		config: data.link.config.clone(),
		epoch_changes: data.link.epoch_changes.clone(),
		mutator: Arc::new(|_, _| ()),
	};

	let genesis_header = client.header(&BlockId::Number(0)).unwrap().unwrap();

	// we should have no issue importing this block
	let b1 = propose_and_import_block(
		&genesis_header,
		Some(999),
		&mut proposer_factory,
		&mut block_import,
	);

	let b1 = client.header(&BlockId::Hash(b1)).unwrap().unwrap();

	// we should fail to import this block since the slot number didn't increase.
	// we will panic due to the `PanickingBlockImport` defined above.
	propose_and_import_block(
		&b1,
		Some(999),
		&mut proposer_factory,
		&mut block_import,
	);
}
