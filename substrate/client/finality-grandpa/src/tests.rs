// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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

//! Tests and test helpers for GRANDPA.

use super::*;
use environment::HasVoted;
use sc_network_test::{
	Block, DummySpecialization, Hash, TestNetFactory, BlockImportAdapter, Peer,
	PeersClient, PassThroughVerifier,
};
use sc_network::config::{ProtocolConfig, Roles, BoxFinalityProofRequestBuilder};
use parking_lot::Mutex;
use futures_timer::Delay;
use futures03::{StreamExt as _, TryStreamExt as _};
use tokio::runtime::current_thread;
use sp_keyring::Ed25519Keyring;
use sc_client::LongestChain;
use sc_client_api::backend::TransactionFor;
use sp_blockchain::Result;
use sp_api::{ApiRef, ApiErrorExt, Core, RuntimeVersion, ApiExt, StorageProof, ProvideRuntimeApi};
use substrate_test_runtime_client::runtime::BlockNumber;
use sp_consensus::{
	BlockOrigin, ForkChoiceStrategy, ImportedAux, BlockImportParams, ImportResult, BlockImport,
	import_queue::{BoxJustificationImport, BoxFinalityProofImport},
};
use std::collections::{HashMap, HashSet};
use std::result;
use parity_scale_codec::Decode;
use sp_runtime::traits::{Header as HeaderT, HasherFor};
use sp_runtime::generic::{BlockId, DigestItem};
use sp_core::{H256, NativeOrEncoded, ExecutionContext, crypto::Public};
use sp_finality_grandpa::{GRANDPA_ENGINE_ID, AuthorityList, GrandpaApi};
use sp_state_machine::{InMemoryBackend, prove_read, read_proof_check};
use std::{pin::Pin, task};

use authorities::AuthoritySet;
use finality_proof::{
	FinalityProofProvider, AuthoritySetForFinalityProver, AuthoritySetForFinalityChecker,
};
use consensus_changes::ConsensusChanges;

type PeerData =
	Mutex<
		Option<
			LinkHalf<
				substrate_test_runtime_client::Backend,
				substrate_test_runtime_client::Executor,
				Block,
				substrate_test_runtime_client::runtime::RuntimeApi,
				LongestChain<substrate_test_runtime_client::Backend, Block>
			>
		>
	>;
type GrandpaPeer = Peer<PeerData, DummySpecialization>;

struct GrandpaTestNet {
	peers: Vec<GrandpaPeer>,
	test_config: TestApi,
}

impl GrandpaTestNet {
	fn new(test_config: TestApi, n_peers: usize) -> Self {
		let mut net = GrandpaTestNet {
			peers: Vec::with_capacity(n_peers),
			test_config,
		};
		let config = Self::default_config();
		for _ in 0..n_peers {
			net.add_full_peer(&config);
		}
		net
	}
}

impl TestNetFactory for GrandpaTestNet {
	type Specialization = DummySpecialization;
	type Verifier = PassThroughVerifier;
	type PeerData = PeerData;

	/// Create new test network with peers and given config.
	fn from_config(_config: &ProtocolConfig) -> Self {
		GrandpaTestNet {
			peers: Vec::new(),
			test_config: Default::default(),
		}
	}

	fn default_config() -> ProtocolConfig {
		// the authority role ensures gossip hits all nodes here.
		let mut config = ProtocolConfig::default();
		config.roles = Roles::AUTHORITY;
		config
	}

	fn make_verifier(
		&self,
		_client: PeersClient,
		_cfg: &ProtocolConfig,
		_: &PeerData,
	) -> Self::Verifier {
		PassThroughVerifier(false) // use non-instant finality.
	}

	fn make_block_import<Transaction>(&self, client: PeersClient)
		-> (
			BlockImportAdapter<Transaction>,
			Option<BoxJustificationImport<Block>>,
			Option<BoxFinalityProofImport<Block>>,
			Option<BoxFinalityProofRequestBuilder<Block>>,
			PeerData,
		)
	{
		match client {
			PeersClient::Full(ref client, ref backend) => {
				let (import, link) = block_import(
					client.clone(),
					&self.test_config,
					LongestChain::new(backend.clone()),
				).expect("Could not create block import for fresh peer.");
				let justification_import = Box::new(import.clone());
				(
					BlockImportAdapter::new_full(import),
					Some(justification_import),
					None,
					None,
					Mutex::new(Some(link)),
				)
			},
			PeersClient::Light(ref client, ref backend) => {
				use crate::light_import::tests::light_block_import_without_justifications;

				let authorities_provider = Arc::new(self.test_config.clone());
				// forbid direct finalization using justification that cames with the block
				// => light clients will try to fetch finality proofs
				let import = light_block_import_without_justifications(
					client.clone(),
					backend.clone(),
					&self.test_config,
					authorities_provider,
				).expect("Could not create block import for fresh peer.");
				let finality_proof_req_builder = import.0.create_finality_proof_request_builder();
				let proof_import = Box::new(import.clone());
				(
					BlockImportAdapter::new_light(import),
					None,
					Some(proof_import),
					Some(finality_proof_req_builder),
					Mutex::new(None),
				)
			},
		}
	}

	fn make_finality_proof_provider(
		&self,
		client: PeersClient
	) -> Option<Arc<dyn sc_network::FinalityProofProvider<Block>>> {
		match client {
			PeersClient::Full(_, ref backend)  => {
				let authorities_provider = Arc::new(self.test_config.clone());
				Some(Arc::new(FinalityProofProvider::new(backend.clone(), authorities_provider)))
			},
			PeersClient::Light(_, _) => None,
		}
	}

	fn peer(&mut self, i: usize) -> &mut GrandpaPeer {
		&mut self.peers[i]
	}

	fn peers(&self) -> &Vec<GrandpaPeer> {
		&self.peers
	}

	fn mut_peers<F: FnOnce(&mut Vec<GrandpaPeer>)>(&mut self, closure: F) {
		closure(&mut self.peers);
	}
}

#[derive(Clone)]
struct Exit;

impl futures03::Future for Exit {
	type Output = ();

	fn poll(self: Pin<&mut Self>, _: &mut task::Context) -> task::Poll<()> {
		task::Poll::Pending
	}
}

#[derive(Default, Clone)]
pub(crate) struct TestApi {
	genesis_authorities: AuthorityList,
}

impl TestApi {
	pub fn new(genesis_authorities: AuthorityList) -> Self {
		TestApi {
			genesis_authorities,
		}
	}
}

pub(crate) struct RuntimeApi {
	inner: TestApi,
}

impl ProvideRuntimeApi<Block> for TestApi {
	type Api = RuntimeApi;

	fn runtime_api<'a>(&'a self) -> ApiRef<'a, Self::Api> {
		RuntimeApi { inner: self.clone() }.into()
	}
}

impl Core<Block> for RuntimeApi {
	fn Core_version_runtime_api_impl(
		&self,
		_: &BlockId<Block>,
		_: ExecutionContext,
		_: Option<()>,
		_: Vec<u8>,
	) -> Result<NativeOrEncoded<RuntimeVersion>> {
		unimplemented!("Not required for testing!")
	}

	fn Core_execute_block_runtime_api_impl(
		&self,
		_: &BlockId<Block>,
		_: ExecutionContext,
		_: Option<Block>,
		_: Vec<u8>,
	) -> Result<NativeOrEncoded<()>> {
		unimplemented!("Not required for testing!")
	}

	fn Core_initialize_block_runtime_api_impl(
		&self,
		_: &BlockId<Block>,
		_: ExecutionContext,
		_: Option<&<Block as BlockT>::Header>,
		_: Vec<u8>,
	) -> Result<NativeOrEncoded<()>> {
		unimplemented!("Not required for testing!")
	}
}

impl ApiErrorExt for RuntimeApi {
	type Error = sp_blockchain::Error;
}

impl ApiExt<Block> for RuntimeApi {
	type StateBackend = <
		substrate_test_runtime_client::Backend as sc_client_api::backend::Backend<Block>
	>::State;

	fn map_api_result<F: FnOnce(&Self) -> result::Result<R, E>, R, E>(
		&self,
		_: F
	) -> result::Result<R, E> {
		unimplemented!("Not required for testing!")
	}

	fn runtime_version_at(&self, _: &BlockId<Block>) -> Result<RuntimeVersion> {
		unimplemented!("Not required for testing!")
	}

	fn record_proof(&mut self) {
		unimplemented!("Not required for testing!")
	}

	fn extract_proof(&mut self) -> Option<StorageProof> {
		unimplemented!("Not required for testing!")
	}

	fn into_storage_changes(
		&self,
		_: &Self::StateBackend,
		_: Option<&sp_api::ChangesTrieState<sp_api::HasherFor<Block>, sp_api::NumberFor<Block>>>,
		_: <Block as sp_api::BlockT>::Hash,
	) -> std::result::Result<sp_api::StorageChanges<Self::StateBackend, Block>, String>
		where Self: Sized
	{
		unimplemented!("Not required for testing!")
	}
}

impl GrandpaApi<Block> for RuntimeApi {
	fn GrandpaApi_grandpa_authorities_runtime_api_impl(
		&self,
		_: &BlockId<Block>,
		_: ExecutionContext,
		_: Option<()>,
		_: Vec<u8>,
	) -> Result<NativeOrEncoded<AuthorityList>> {
		Ok(self.inner.genesis_authorities.clone()).map(NativeOrEncoded::Native)
	}
}

impl GenesisAuthoritySetProvider<Block> for TestApi {
	fn get(&self) -> Result<AuthorityList> {
		Ok(self.genesis_authorities.clone())
	}
}

impl AuthoritySetForFinalityProver<Block> for TestApi {
	fn authorities(&self, _block: &BlockId<Block>) -> Result<AuthorityList> {
		Ok(self.genesis_authorities.clone())
	}

	fn prove_authorities(&self, block: &BlockId<Block>) -> Result<StorageProof> {
		let authorities = self.authorities(block)?;
		let backend = <InMemoryBackend<HasherFor<Block>>>::from(vec![
			(None, vec![(b"authorities".to_vec(), Some(authorities.encode()))])
		]);
		let proof = prove_read(backend, vec![b"authorities"])
			.expect("failure proving read from in-memory storage backend");
		Ok(proof)
	}
}

impl AuthoritySetForFinalityChecker<Block> for TestApi {
	fn check_authorities_proof(
		&self,
		_hash: <Block as BlockT>::Hash,
		header: <Block as BlockT>::Header,
		proof: StorageProof,
	) -> Result<AuthorityList> {
		let results = read_proof_check::<HasherFor<Block>, _>(
			*header.state_root(), proof, vec![b"authorities"]
		)
			.expect("failure checking read proof for authorities");
		let encoded = results.get(&b"authorities"[..])
			.expect("returned map must contain all proof keys")
			.as_ref()
			.expect("authorities in proof is None");
		let authorities = Decode::decode(&mut &encoded[..])
			.expect("failure decoding authorities read from proof");
		Ok(authorities)
	}
}

const TEST_GOSSIP_DURATION: Duration = Duration::from_millis(500);

fn make_ids(keys: &[Ed25519Keyring]) -> AuthorityList {
	keys.iter().map(|key| key.clone().public().into()).map(|id| (id, 1)).collect()
}

fn create_keystore(authority: Ed25519Keyring) -> (KeyStorePtr, tempfile::TempDir) {
	let keystore_path = tempfile::tempdir().expect("Creates keystore path");
	let keystore = sc_keystore::Store::open(keystore_path.path(), None).expect("Creates keystore");
	keystore.write().insert_ephemeral_from_seed::<AuthorityPair>(&authority.to_seed())
		.expect("Creates authority key");

	(keystore, keystore_path)
}

// run the voters to completion. provide a closure to be invoked after
// the voters are spawned but before blocking on them.
fn run_to_completion_with<F>(
	runtime: &mut current_thread::Runtime,
	threads_pool: &futures03::executor::ThreadPool,
	blocks: u64,
	net: Arc<Mutex<GrandpaTestNet>>,
	peers: &[Ed25519Keyring],
	with: F,
) -> u64 where
	F: FnOnce(current_thread::Handle) -> Option<Box<dyn Future<Item=(), Error=()>>>
{
	use parking_lot::RwLock;

	let mut wait_for = Vec::new();

	let highest_finalized = Arc::new(RwLock::new(0));

	if let Some(f) = (with)(runtime.handle()) {
		wait_for.push(f);
	};

	let mut keystore_paths = Vec::new();
	for (peer_id, key) in peers.iter().enumerate() {
		let (keystore, keystore_path) = create_keystore(*key);
		keystore_paths.push(keystore_path);

		let highest_finalized = highest_finalized.clone();
		let (client, net_service, link) = {
			let net = net.lock();
			// temporary needed for some reason
			let link = net.peers[peer_id].data.lock().take().expect("link initialized at startup; qed");
			(
				net.peers[peer_id].client().clone(),
				net.peers[peer_id].network_service().clone(),
				link,
			)
		};

		wait_for.push(
			Box::new(
				client.finality_notification_stream()
					.map(|v| Ok::<_, ()>(v)).compat()
					.take_while(move |n| {
						let mut highest_finalized = highest_finalized.write();
						if *n.header.number() > *highest_finalized {
							*highest_finalized = *n.header.number();
						}
						Ok(n.header.number() < &blocks)
					})
					.collect()
					.map(|_| ())
			)
		);

		fn assert_send<T: Send>(_: &T) { }

		let grandpa_params = GrandpaParams {
			config: Config {
				gossip_duration: TEST_GOSSIP_DURATION,
				justification_period: 32,
				keystore: Some(keystore),
				name: Some(format!("peer#{}", peer_id)),
				is_authority: true,
				observer_enabled: true,
			},
			link: link,
			network: net_service,
			inherent_data_providers: InherentDataProviders::new(),
			on_exit: Exit,
			telemetry_on_connect: None,
			voting_rule: (),
			executor: threads_pool.clone(),
		};
		let voter = run_grandpa_voter(grandpa_params).expect("all in order with client and network");

		assert_send(&voter);

		runtime.spawn(voter);
	}

	// wait for all finalized on each.
	let wait_for = ::futures::future::join_all(wait_for)
		.map(|_| ())
		.map_err(|_| ());

	let drive_to_completion = futures::future::poll_fn(|| { net.lock().poll(); Ok(Async::NotReady) });
	let _ = runtime.block_on(wait_for.select(drive_to_completion).map_err(|_| ())).unwrap();

	let highest_finalized = *highest_finalized.read();
	highest_finalized
}

fn run_to_completion(
	runtime: &mut current_thread::Runtime,
	threads_pool: &futures03::executor::ThreadPool,
	blocks: u64,
	net: Arc<Mutex<GrandpaTestNet>>,
	peers: &[Ed25519Keyring]
) -> u64 {
	run_to_completion_with(runtime, threads_pool, blocks, net, peers, |_| None)
}

fn add_scheduled_change(block: &mut Block, change: ScheduledChange<BlockNumber>) {
	block.header.digest_mut().push(DigestItem::Consensus(
		GRANDPA_ENGINE_ID,
		sp_finality_grandpa::ConsensusLog::ScheduledChange(change).encode(),
	));
}

fn add_forced_change(
	block: &mut Block,
	median_last_finalized: BlockNumber,
	change: ScheduledChange<BlockNumber>,
) {
	block.header.digest_mut().push(DigestItem::Consensus(
		GRANDPA_ENGINE_ID,
		sp_finality_grandpa::ConsensusLog::ForcedChange(median_last_finalized, change).encode(),
	));
}

#[test]
fn finalize_3_voters_no_observers() {
	let _ = env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let peers = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let voters = make_ids(peers);

	let mut net = GrandpaTestNet::new(TestApi::new(voters), 3);
	net.peer(0).push_blocks(20, false);
	net.block_until_sync(&mut runtime);

	for i in 0..3 {
		assert_eq!(net.peer(i).client().info().best_number, 20,
			"Peer #{} failed to sync", i);
	}

	let net = Arc::new(Mutex::new(net));
	run_to_completion(&mut runtime, &threads_pool, 20, net.clone(), peers);

	// normally there's no justification for finalized blocks
	assert!(
		net.lock().peer(0).client().justification(&BlockId::Number(20)).unwrap().is_none(),
		"Extra justification for block#1",
	);
}

#[test]
fn finalize_3_voters_1_full_observer() {
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	let peers = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let voters = make_ids(peers);

	let mut net = GrandpaTestNet::new(TestApi::new(voters), 4);
	net.peer(0).push_blocks(20, false);
	net.block_until_sync(&mut runtime);

	let net = Arc::new(Mutex::new(net));
	let mut finality_notifications = Vec::new();

	let all_peers = peers.iter()
		.cloned()
		.map(Some)
		.chain(std::iter::once(None));

	let mut keystore_paths = Vec::new();

	let mut voters = Vec::new();

	for (peer_id, local_key) in all_peers.enumerate() {
		let (client, net_service, link) = {
			let net = net.lock();
			let link = net.peers[peer_id].data.lock().take().expect("link initialized at startup; qed");
			(
				net.peers[peer_id].client().clone(),
				net.peers[peer_id].network_service().clone(),
				link,
			)
		};
		finality_notifications.push(
			client.finality_notification_stream()
				.map(|v| Ok::<_, ()>(v)).compat()
				.take_while(|n| Ok(n.header.number() < &20))
				.for_each(move |_| Ok(()))
		);

		let keystore = if let Some(local_key) = local_key {
			let (keystore, keystore_path) = create_keystore(local_key);
			keystore_paths.push(keystore_path);
			Some(keystore)
		} else {
			None
		};

		let grandpa_params = GrandpaParams {
			config: Config {
				gossip_duration: TEST_GOSSIP_DURATION,
				justification_period: 32,
				keystore,
				name: Some(format!("peer#{}", peer_id)),
				is_authority: true,
				observer_enabled: true,
			},
			link: link,
			network: net_service,
			inherent_data_providers: InherentDataProviders::new(),
			on_exit: Exit,
			telemetry_on_connect: None,
			voting_rule: (),
			executor: threads_pool.clone(),
		};

		voters.push(run_grandpa_voter(grandpa_params).expect("all in order with client and network"));
	}

	for voter in voters {
		runtime.spawn(voter);
	}

	// wait for all finalized on each.
	let wait_for = futures::future::join_all(finality_notifications)
		.map(|_| ())
		.map_err(|_| ());

	let drive_to_completion = futures::future::poll_fn(|| { net.lock().poll(); Ok(Async::NotReady) });
	let _ = runtime.block_on(wait_for.select(drive_to_completion).map_err(|_| ())).unwrap();
}

#[test]
fn transition_3_voters_twice_1_full_observer() {
	let _ = env_logger::try_init();
	let peers_a = &[
		Ed25519Keyring::Alice,
		Ed25519Keyring::Bob,
		Ed25519Keyring::Charlie,
	];

	let peers_b = &[
		Ed25519Keyring::Dave,
		Ed25519Keyring::Eve,
		Ed25519Keyring::Ferdie,
	];

	let peers_c = &[
		Ed25519Keyring::Alice,
		Ed25519Keyring::Eve,
		Ed25519Keyring::Two,
	];

	let observer = &[Ed25519Keyring::One];

	let genesis_voters = make_ids(peers_a);

	let api = TestApi::new(genesis_voters);
	let net = Arc::new(Mutex::new(GrandpaTestNet::new(api, 8)));

	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	net.lock().peer(0).push_blocks(1, false);
	net.lock().block_until_sync(&mut runtime);

	for (i, peer) in net.lock().peers().iter().enumerate() {
		let full_client = peer.client().as_full().expect("only full clients are used in test");
		assert_eq!(full_client.chain_info().best_number, 1,
					"Peer #{} failed to sync", i);

		let set: AuthoritySet<Hash, BlockNumber> = crate::aux_schema::load_authorities(&*full_client).unwrap();

		assert_eq!(set.current(), (0, make_ids(peers_a).as_slice()));
		assert_eq!(set.pending_changes().count(), 0);
	}

	{
		let net = net.clone();
		let client = net.lock().peers[0].client().clone();
		let peers_c = peers_c.clone();

		// wait for blocks to be finalized before generating new ones
		let block_production = client.finality_notification_stream()
			.map(|v| Ok::<_, ()>(v)).compat()
			.take_while(|n| Ok(n.header.number() < &30))
			.for_each(move |n| {
				match n.header.number() {
					1 => {
						// first 14 blocks.
						net.lock().peer(0).push_blocks(13, false);
					},
					14 => {
						// generate transition at block 15, applied at 20.
						net.lock().peer(0).generate_blocks(1, BlockOrigin::File, |builder| {
							let mut block = builder.build().unwrap().block;
							add_scheduled_change(&mut block, ScheduledChange {
								next_authorities: make_ids(peers_b),
								delay: 4,
							});

							block
						});
						net.lock().peer(0).push_blocks(5, false);
					},
					20 => {
						// at block 21 we do another transition, but this time instant.
						// add more until we have 30.
						net.lock().peer(0).generate_blocks(1, BlockOrigin::File, |builder| {
							let mut block = builder.build().unwrap().block;
							add_scheduled_change(&mut block, ScheduledChange {
								next_authorities: make_ids(&peers_c),
								delay: 0,
							});

							block
						});
						net.lock().peer(0).push_blocks(9, false);
					},
					_ => {},
				}

				Ok(())
			});

		runtime.spawn(block_production);
	}

	let mut finality_notifications = Vec::new();
	let all_peers = peers_a.iter()
		.chain(peers_b)
		.chain(peers_c)
		.chain(observer)
		.cloned()
		.collect::<HashSet<_>>() // deduplicate
		.into_iter()
		.enumerate();

	let mut keystore_paths = Vec::new();
	for (peer_id, local_key) in all_peers {
		let (keystore, keystore_path) = create_keystore(local_key);
		keystore_paths.push(keystore_path);

		let (client, net_service, link) = {
			let net = net.lock();
			let link = net.peers[peer_id].data.lock().take().expect("link initialized at startup; qed");
			(
				net.peers[peer_id].client().clone(),
				net.peers[peer_id].network_service().clone(),
				link,
			)
		};

		finality_notifications.push(
			client.finality_notification_stream()
				.map(|v| Ok::<_, ()>(v)).compat()
				.take_while(|n| Ok(n.header.number() < &30))
				.for_each(move |_| Ok(()))
				.map(move |()| {
					let full_client = client.as_full().expect("only full clients are used in test");
					let set: AuthoritySet<Hash, BlockNumber> = crate::aux_schema::load_authorities(&*full_client).unwrap();

					assert_eq!(set.current(), (2, make_ids(peers_c).as_slice()));
					assert_eq!(set.pending_changes().count(), 0);
				})
		);

		let grandpa_params = GrandpaParams {
			config: Config {
				gossip_duration: TEST_GOSSIP_DURATION,
				justification_period: 32,
				keystore: Some(keystore),
				name: Some(format!("peer#{}", peer_id)),
				is_authority: true,
				observer_enabled: true,
			},
			link: link,
			network: net_service,
			inherent_data_providers: InherentDataProviders::new(),
			on_exit: Exit,
			telemetry_on_connect: None,
			voting_rule: (),
			executor: threads_pool.clone(),
		};
		let voter = run_grandpa_voter(grandpa_params).expect("all in order with client and network");

		runtime.spawn(voter);
	}

	// wait for all finalized on each.
	let wait_for = ::futures::future::join_all(finality_notifications)
		.map(|_| ())
		.map_err(|_| ());

	let drive_to_completion = futures::future::poll_fn(|| { net.lock().poll(); Ok(Async::NotReady) });
	let _ = runtime.block_on(wait_for.select(drive_to_completion).map_err(|_| ())).unwrap();
}

#[test]
fn justification_is_emitted_when_consensus_data_changes() {
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let peers = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let mut net = GrandpaTestNet::new(TestApi::new(make_ids(peers)), 3);

	// import block#1 WITH consensus data change
	let new_authorities = vec![sp_consensus_babe::AuthorityId::from_slice(&[42; 32])];
	net.peer(0).push_authorities_change_block(new_authorities);
	net.block_until_sync(&mut runtime);
	let net = Arc::new(Mutex::new(net));
	run_to_completion(&mut runtime, &threads_pool, 1, net.clone(), peers);

	// ... and check that there's justification for block#1
	assert!(net.lock().peer(0).client().justification(&BlockId::Number(1)).unwrap().is_some(),
		"Missing justification for block#1");
}

#[test]
fn justification_is_generated_periodically() {
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let peers = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let voters = make_ids(peers);

	let mut net = GrandpaTestNet::new(TestApi::new(voters), 3);
	net.peer(0).push_blocks(32, false);
	net.block_until_sync(&mut runtime);

	let net = Arc::new(Mutex::new(net));
	run_to_completion(&mut runtime, &threads_pool, 32, net.clone(), peers);

	// when block#32 (justification_period) is finalized, justification
	// is required => generated
	for i in 0..3 {
		assert!(net.lock().peer(i).client().justification(&BlockId::Number(32)).unwrap().is_some());
	}
}

#[test]
fn consensus_changes_works() {
	let mut changes = ConsensusChanges::<H256, u64>::empty();

	// pending changes are not finalized
	changes.note_change((10, H256::from_low_u64_be(1)));
	assert_eq!(changes.finalize((5, H256::from_low_u64_be(5)), |_| Ok(None)).unwrap(), (false, false));

	// no change is selected from competing pending changes
	changes.note_change((1, H256::from_low_u64_be(1)));
	changes.note_change((1, H256::from_low_u64_be(101)));
	assert_eq!(changes.finalize((10, H256::from_low_u64_be(10)), |_| Ok(Some(H256::from_low_u64_be(1001)))).unwrap(), (true, false));

	// change is selected from competing pending changes
	changes.note_change((1, H256::from_low_u64_be(1)));
	changes.note_change((1, H256::from_low_u64_be(101)));
	assert_eq!(changes.finalize((10, H256::from_low_u64_be(10)), |_| Ok(Some(H256::from_low_u64_be(1)))).unwrap(), (true, true));
}

#[test]
fn sync_justifications_on_change_blocks() {
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let peers_a = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let peers_b = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob];
	let voters = make_ids(peers_b);

	// 4 peers, 3 of them are authorities and participate in grandpa
	let api = TestApi::new(voters);
	let mut net = GrandpaTestNet::new(api, 4);

	// add 20 blocks
	net.peer(0).push_blocks(20, false);

	// at block 21 we do add a transition which is instant
	net.peer(0).generate_blocks(1, BlockOrigin::File, |builder| {
		let mut block = builder.build().unwrap().block;
		add_scheduled_change(&mut block, ScheduledChange {
			next_authorities: make_ids(peers_b),
			delay: 0,
		});
		block
	});

	// add more blocks on top of it (until we have 25)
	net.peer(0).push_blocks(4, false);
	net.block_until_sync(&mut runtime);

	for i in 0..4 {
		assert_eq!(net.peer(i).client().info().best_number, 25,
			"Peer #{} failed to sync", i);
	}

	let net = Arc::new(Mutex::new(net));
	run_to_completion(&mut runtime, &threads_pool, 25, net.clone(), peers_a);

	// the first 3 peers are grandpa voters and therefore have already finalized
	// block 21 and stored a justification
	for i in 0..3 {
		assert!(net.lock().peer(i).client().justification(&BlockId::Number(21)).unwrap().is_some());
	}

	// the last peer should get the justification by syncing from other peers
	runtime.block_on(futures::future::poll_fn(move || -> std::result::Result<_, ()> {
		if net.lock().peer(3).client().justification(&BlockId::Number(21)).unwrap().is_none() {
			net.lock().poll();
			Ok(Async::NotReady)
		} else {
			Ok(Async::Ready(()))
		}
	})).unwrap()
}

#[test]
fn finalizes_multiple_pending_changes_in_order() {
	let _ = env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	let peers_a = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let peers_b = &[Ed25519Keyring::Dave, Ed25519Keyring::Eve, Ed25519Keyring::Ferdie];
	let peers_c = &[Ed25519Keyring::Dave, Ed25519Keyring::Alice, Ed25519Keyring::Bob];

	let all_peers = &[
		Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie,
		Ed25519Keyring::Dave, Ed25519Keyring::Eve, Ed25519Keyring::Ferdie,
	];
	let genesis_voters = make_ids(peers_a);

	// 6 peers, 3 of them are authorities and participate in grandpa from genesis
	let api = TestApi::new(genesis_voters);
	let mut net = GrandpaTestNet::new(api, 6);

	// add 20 blocks
	net.peer(0).push_blocks(20, false);

	// at block 21 we do add a transition which is instant
	net.peer(0).generate_blocks(1, BlockOrigin::File, |builder| {
		let mut block = builder.build().unwrap().block;
		add_scheduled_change(&mut block, ScheduledChange {
			next_authorities: make_ids(peers_b),
			delay: 0,
		});
		block
	});

	// add more blocks on top of it (until we have 25)
	net.peer(0).push_blocks(4, false);

	// at block 26 we add another which is enacted at block 30
	net.peer(0).generate_blocks(1, BlockOrigin::File, |builder| {
		let mut block = builder.build().unwrap().block;
		add_scheduled_change(&mut block, ScheduledChange {
			next_authorities: make_ids(peers_c),
			delay: 4,
		});
		block
	});

	// add more blocks on top of it (until we have 30)
	net.peer(0).push_blocks(4, false);

	net.block_until_sync(&mut runtime);

	// all peers imported both change blocks
	for i in 0..6 {
		assert_eq!(net.peer(i).client().info().best_number, 30,
			"Peer #{} failed to sync", i);
	}

	let net = Arc::new(Mutex::new(net));
	run_to_completion(&mut runtime, &threads_pool, 30, net.clone(), all_peers);
}

#[test]
fn force_change_to_new_set() {
	let _ = env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	// two of these guys are offline.
	let genesis_authorities = &[
		Ed25519Keyring::Alice,
		Ed25519Keyring::Bob,
		Ed25519Keyring::Charlie,
		Ed25519Keyring::One,
		Ed25519Keyring::Two,
	];
	let peers_a = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let api = TestApi::new(make_ids(genesis_authorities));

	let voters = make_ids(peers_a);
	let net = GrandpaTestNet::new(api, 3);
	let net = Arc::new(Mutex::new(net));

	net.lock().peer(0).generate_blocks(1, BlockOrigin::File, |builder| {
		let mut block = builder.build().unwrap().block;

		// add a forced transition at block 12.
		add_forced_change(&mut block, 0, ScheduledChange {
			next_authorities: voters.clone(),
			delay: 10,
		});

		// add a normal transition too to ensure that forced changes take priority.
		add_scheduled_change(&mut block, ScheduledChange {
			next_authorities: make_ids(genesis_authorities),
			delay: 5,
		});

		block
	});

	net.lock().peer(0).push_blocks(25, false);
	net.lock().block_until_sync(&mut runtime);

	for (i, peer) in net.lock().peers().iter().enumerate() {
		assert_eq!(peer.client().info().best_number, 26,
				"Peer #{} failed to sync", i);

		let full_client = peer.client().as_full().expect("only full clients are used in test");
		let set: AuthoritySet<Hash, BlockNumber> = crate::aux_schema::load_authorities(&*full_client).unwrap();

		assert_eq!(set.current(), (1, voters.as_slice()));
		assert_eq!(set.pending_changes().count(), 0);
	}

	// it will only finalize if the forced transition happens.
	// we add_blocks after the voters are spawned because otherwise
	// the link-halfs have the wrong AuthoritySet
	run_to_completion(&mut runtime, &threads_pool, 25, net, peers_a);
}

#[test]
fn allows_reimporting_change_blocks() {
	let peers_a = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let peers_b = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob];
	let voters = make_ids(peers_a);
	let api = TestApi::new(voters);
	let mut net = GrandpaTestNet::new(api.clone(), 3);

	let client = net.peer(0).client().clone();
	let (mut block_import, ..) = net.make_block_import::<
		TransactionFor<substrate_test_runtime_client::Backend, Block>
	>(
		client.clone(),
	);

	let full_client = client.as_full().unwrap();
	let builder = full_client.new_block_at(&BlockId::Number(0), Default::default(), false).unwrap();
	let mut block = builder.build().unwrap().block;
	add_scheduled_change(&mut block, ScheduledChange {
		next_authorities: make_ids(peers_b),
		delay: 0,
	});

	let block = || {
		let block = block.clone();
		BlockImportParams {
			origin: BlockOrigin::File,
			header: block.header,
			justification: None,
			post_digests: Vec::new(),
			body: Some(block.extrinsics),
			storage_changes: None,
			finalized: false,
			auxiliary: Vec::new(),
			fork_choice: ForkChoiceStrategy::LongestChain,
			allow_missing_state: false,
			import_existing: false,
		}
	};

	assert_eq!(
		block_import.import_block(block(), HashMap::new()).unwrap(),
		ImportResult::Imported(ImportedAux {
			needs_justification: true,
			clear_justification_requests: false,
			bad_justification: false,
			needs_finality_proof: false,
			is_new_best: true,
			header_only: false,
		}),
	);

	assert_eq!(
		block_import.import_block(block(), HashMap::new()).unwrap(),
		ImportResult::AlreadyInChain
	);
}

#[test]
fn test_bad_justification() {
	let peers_a = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let peers_b = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob];
	let voters = make_ids(peers_a);
	let api = TestApi::new(voters);
	let mut net = GrandpaTestNet::new(api.clone(), 3);

	let client = net.peer(0).client().clone();
	let (mut block_import, ..) = net.make_block_import::<
		TransactionFor<substrate_test_runtime_client::Backend, Block>
	>(
		client.clone(),
	);

	let full_client = client.as_full().expect("only full clients are used in test");
	let builder = full_client.new_block_at(&BlockId::Number(0), Default::default(), false).unwrap();
	let mut block = builder.build().unwrap().block;

	add_scheduled_change(&mut block, ScheduledChange {
		next_authorities: make_ids(peers_b),
		delay: 0,
	});

	let block = || {
		let block = block.clone();
		BlockImportParams {
			origin: BlockOrigin::File,
			header: block.header,
			justification: Some(Vec::new()),
			post_digests: Vec::new(),
			body: Some(block.extrinsics),
			storage_changes: None,
			finalized: false,
			auxiliary: Vec::new(),
			fork_choice: ForkChoiceStrategy::LongestChain,
			allow_missing_state: false,
			import_existing: false,
		}
	};

	assert_eq!(
		block_import.import_block(block(), HashMap::new()).unwrap(),
		ImportResult::Imported(ImportedAux {
			needs_justification: true,
			clear_justification_requests: false,
			bad_justification: true,
			is_new_best: true,
			..Default::default()
		}),
	);

	assert_eq!(
		block_import.import_block(block(), HashMap::new()).unwrap(),
		ImportResult::AlreadyInChain
	);
}

#[test]
fn voter_persists_its_votes() {
	use std::iter::FromIterator;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use futures::future;
	use futures::sync::mpsc;

	let _ = env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	// we have two authorities but we'll only be running the voter for alice
	// we are going to be listening for the prevotes it casts
	let peers = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob];
	let voters = make_ids(peers);

	// alice has a chain with 20 blocks
	let mut net = GrandpaTestNet::new(TestApi::new(voters.clone()), 2);
	net.peer(0).push_blocks(20, false);
	net.block_until_sync(&mut runtime);

	assert_eq!(net.peer(0).client().info().best_number, 20,
			   "Peer #{} failed to sync", 0);


	let peer = net.peer(0);
	let client = peer.client().clone();
	let net = Arc::new(Mutex::new(net));

	// channel between the voter and the main controller.
	// sending a message on the `voter_tx` restarts the voter.
	let (voter_tx, voter_rx) = mpsc::unbounded::<()>();

	let mut keystore_paths = Vec::new();

	// startup a grandpa voter for alice but also listen for messages on a
	// channel. whenever a message is received the voter is restarted. when the
	// sender is dropped the voter is stopped.
	{
		let (keystore, keystore_path) = create_keystore(peers[0]);
		keystore_paths.push(keystore_path);

		struct ResettableVoter {
			voter: Box<dyn Future<Item = (), Error = ()> + Send>,
			voter_rx: mpsc::UnboundedReceiver<()>,
			net: Arc<Mutex<GrandpaTestNet>>,
			client: PeersClient,
			keystore: KeyStorePtr,
			threads_pool: futures03::executor::ThreadPool,
		}

		impl Future for ResettableVoter {
			type Item = ();
			type Error = ();

			fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
				match self.voter.poll() {
					Ok(Async::Ready(())) | Err(_) => panic!("error in the voter"),
					Ok(Async::NotReady) => {},
				}

				match self.voter_rx.poll() {
					Err(_) | Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
					Ok(Async::NotReady) => {}
					Ok(Async::Ready(Some(()))) => {
						let (_block_import, _, _, _, link) =
							self.net.lock()
									.make_block_import::<
										TransactionFor<substrate_test_runtime_client::Backend, Block>
									>(self.client.clone());
						let link = link.lock().take().unwrap();

						let grandpa_params = GrandpaParams {
							config: Config {
								gossip_duration: TEST_GOSSIP_DURATION,
								justification_period: 32,
								keystore: Some(self.keystore.clone()),
								name: Some(format!("peer#{}", 0)),
								is_authority: true,
								observer_enabled: true,
							},
							link,
							network: self.net.lock().peers[0].network_service().clone(),
							inherent_data_providers: InherentDataProviders::new(),
							on_exit: Exit,
							telemetry_on_connect: None,
							voting_rule: VotingRulesBuilder::default().build(),
							executor: self.threads_pool.clone(),
						};

						let voter = run_grandpa_voter(grandpa_params)
							.expect("all in order with client and network")
							.then(move |r| {
								// we need to keep the block_import alive since it owns the
								// sender for the voter commands channel, if that gets dropped
								// then the voter will stop
								drop(_block_import);
								r
							});

						self.voter = Box::new(voter);
						// notify current task in order to poll the voter
						futures::task::current().notify();
					}
				};

				Ok(Async::NotReady)
			}
		}

		// we create a "dummy" voter by setting it to `empty` and triggering the `tx`.
		// this way, the `ResettableVoter` will reset its `voter` field to a value ASAP.
		voter_tx.unbounded_send(()).unwrap();
		runtime.spawn(ResettableVoter {
			voter: Box::new(futures::future::empty()),
			voter_rx,
			net: net.clone(),
			client: client.clone(),
			keystore,
			threads_pool: threads_pool.clone(),
		});
	}

	let (exit_tx, exit_rx) = futures::sync::oneshot::channel::<()>();

	// create the communication layer for bob, but don't start any
	// voter. instead we'll listen for the prevote that alice casts
	// and cast our own manually
	{
		let (keystore, keystore_path) = create_keystore(peers[1]);
		keystore_paths.push(keystore_path);

		let config = Config {
			gossip_duration: TEST_GOSSIP_DURATION,
			justification_period: 32,
			keystore: Some(keystore),
			name: Some(format!("peer#{}", 1)),
			is_authority: true,
			observer_enabled: true,
		};

		let set_state = {
			let (_, _, _, _, link) = net.lock()
				.make_block_import::<
					TransactionFor<substrate_test_runtime_client::Backend, Block>
				>(client);
			let LinkHalf { persistent_data, .. } = link.lock().take().unwrap();
			let PersistentData { set_state, .. } = persistent_data;
			set_state
		};

		let network = communication::NetworkBridge::new(
			net.lock().peers[1].network_service().clone(),
			config.clone(),
			set_state,
			&threads_pool,
			Exit,
		);

		let (round_rx, round_tx) = network.round_communication(
			communication::Round(1),
			communication::SetId(0),
			Arc::new(VoterSet::from_iter(voters)),
			Some(peers[1].pair().into()),
			HasVoted::No,
		);

		let round_tx = Arc::new(Mutex::new(round_tx));
		let exit_tx = Arc::new(Mutex::new(Some(exit_tx)));

		let net = net.clone();
		let state = AtomicUsize::new(0);

		runtime.spawn(round_rx.for_each(move |signed| {
			if state.compare_and_swap(0, 1, Ordering::SeqCst) == 0 {
				// the first message we receive should be a prevote from alice.
				let prevote = match signed.message {
					finality_grandpa::Message::Prevote(prevote) => prevote,
					_ => panic!("voter should prevote."),
				};

				// its chain has 20 blocks and the voter targets 3/4 of the
				// unfinalized chain, so the vote should be for block 15
				assert!(prevote.target_number == 15);

				// we push 20 more blocks to alice's chain
				net.lock().peer(0).push_blocks(20, false);

				let net2 = net.clone();
				let net = net.clone();
				let voter_tx = voter_tx.clone();
				let round_tx = round_tx.clone();

				let interval = futures03::stream::unfold(Delay::new(Duration::from_millis(200)), |delay|
					Box::pin(async move {
						delay.await;
						Some(((), Delay::new(Duration::from_millis(200))))
					})).map(Ok::<_, ()>).compat();

				future::Either::A(interval
					.take_while(move |_| {
						Ok(net2.lock().peer(1).client().info().best_number != 40)
					})
					.for_each(|_| Ok(()))
					.and_then(move |_| {
						let block_30_hash =
							net.lock().peer(0).client().as_full().unwrap().hash(30).unwrap().unwrap();

						// we restart alice's voter
						voter_tx.unbounded_send(()).unwrap();

						// and we push our own prevote for block 30
						let prevote = finality_grandpa::Prevote {
							target_number: 30,
							target_hash: block_30_hash,
						};

						round_tx.lock().start_send(finality_grandpa::Message::Prevote(prevote)).unwrap();
						Ok(())
					}).map_err(|_| panic!()))

			} else if state.compare_and_swap(1, 2, Ordering::SeqCst) == 1 {
				// the next message we receive should be our own prevote
				let prevote = match signed.message {
					finality_grandpa::Message::Prevote(prevote) => prevote,
					_ => panic!("We should receive our own prevote."),
				};

				// targeting block 30
				assert!(prevote.target_number == 30);

				// after alice restarts it should send its previous prevote
				// therefore we won't ever receive it again since it will be a
				// known message on the gossip layer

				future::Either::B(future::ok(()))

			} else if state.compare_and_swap(2, 3, Ordering::SeqCst) == 2 {
				// we then receive a precommit from alice for block 15
				// even though we casted a prevote for block 30
				let precommit = match signed.message {
					finality_grandpa::Message::Precommit(precommit) => precommit,
					_ => panic!("voter should precommit."),
				};

				assert!(precommit.target_number == 15);

				// signal exit
				exit_tx.clone().lock().take().unwrap().send(()).unwrap();

				future::Either::B(future::ok(()))

			} else {
				panic!()
			}

		}).map_err(|_| ()));
	}

	let drive_to_completion = futures::future::poll_fn(|| { net.lock().poll(); Ok(Async::NotReady) });
	let exit = exit_rx.into_future().map(|_| ()).map_err(|_| ());

	runtime.block_on(drive_to_completion.select(exit).map(|_| ()).map_err(|_| ())).unwrap();
}

#[test]
fn finalize_3_voters_1_light_observer() {
	let _ = env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let authorities = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let voters = make_ids(authorities);

	let mut net = GrandpaTestNet::new(TestApi::new(voters), 4);
	net.peer(0).push_blocks(20, false);
	net.block_until_sync(&mut runtime);

	for i in 0..4 {
		assert_eq!(net.peer(i).client().info().best_number, 20,
			"Peer #{} failed to sync", i);
	}

	let net = Arc::new(Mutex::new(net));
	let link = net.lock().peer(3).data.lock().take().expect("link initialized on startup; qed");

	let finality_notifications = net.lock().peer(3).client().finality_notification_stream()
		.map(|v| Ok::<_, ()>(v)).compat()
		.take_while(|n| Ok(n.header.number() < &20))
		.collect();

	run_to_completion_with(&mut runtime, &threads_pool, 20, net.clone(), authorities, |executor| {
		executor.spawn(
			run_grandpa_observer(
				Config {
					gossip_duration: TEST_GOSSIP_DURATION,
					justification_period: 32,
					keystore: None,
					name: Some("observer".to_string()),
					is_authority: false,
					observer_enabled: true,
				},
				link,
				net.lock().peers[3].network_service().clone(),
				Exit,
				threads_pool.clone(),
			).unwrap()
		).unwrap();

		Some(Box::new(finality_notifications.map(|_| ())))
	});
}

#[test]
fn finality_proof_is_fetched_by_light_client_when_consensus_data_changes() {
	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	let peers = &[Ed25519Keyring::Alice];
	let mut net = GrandpaTestNet::new(TestApi::new(make_ids(peers)), 1);
	net.add_light_peer(&GrandpaTestNet::default_config());

	// import block#1 WITH consensus data change. Light client ignores justification
	// && instead fetches finality proof for block #1
	net.peer(0).push_authorities_change_block(vec![sp_consensus_babe::AuthorityId::from_slice(&[42; 32])]);
	let net = Arc::new(Mutex::new(net));
	run_to_completion(&mut runtime, &threads_pool, 1, net.clone(), peers);
	net.lock().block_until_sync(&mut runtime);

	// check that the block#1 is finalized on light client
	runtime.block_on(futures::future::poll_fn(move || -> std::result::Result<_, ()> {
		if net.lock().peer(1).client().info().finalized_number == 1 {
			Ok(Async::Ready(()))
		} else {
			net.lock().poll();
			Ok(Async::NotReady)
		}
	})).unwrap()
}

#[test]
fn empty_finality_proof_is_returned_to_light_client_when_authority_set_is_different() {
	// for debug: to ensure that without forced change light client will sync finality proof
	const FORCE_CHANGE: bool = true;

	let _ = ::env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	// two of these guys are offline.
	let genesis_authorities = if FORCE_CHANGE {
		vec![
			Ed25519Keyring::Alice,
			Ed25519Keyring::Bob,
			Ed25519Keyring::Charlie,
			Ed25519Keyring::One,
			Ed25519Keyring::Two,
		]
	} else {
		vec![
			Ed25519Keyring::Alice,
			Ed25519Keyring::Bob,
			Ed25519Keyring::Charlie,
		]
	};
	let peers_a = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let api = TestApi::new(make_ids(&genesis_authorities));

	let voters = make_ids(peers_a);
	let net = GrandpaTestNet::new(api, 3);
	let net = Arc::new(Mutex::new(net));

	// best is #1
	net.lock().peer(0).generate_blocks(1, BlockOrigin::File, |builder| {
		// add a forced transition at block 5.
		let mut block = builder.build().unwrap().block;
		if FORCE_CHANGE {
			add_forced_change(&mut block, 0, ScheduledChange {
				next_authorities: voters.clone(),
				delay: 3,
			});
		}
		block
	});

	// ensure block#10 enacts authorities set change => justification is generated
	// normally it will reach light client, but because of the forced change, it will not
	net.lock().peer(0).push_blocks(8, false); // best is #9
	net.lock().peer(0).push_authorities_change_block(
		vec![sp_consensus_babe::AuthorityId::from_slice(&[42; 32])]
	); // #10
	net.lock().peer(0).push_blocks(1, false); // best is #11
	net.lock().block_until_sync(&mut runtime);

	// finalize block #11 on full clients
	run_to_completion(&mut runtime, &threads_pool, 11, net.clone(), peers_a);

	// request finalization by light client
	net.lock().add_light_peer(&GrandpaTestNet::default_config());
	net.lock().block_until_sync(&mut runtime);

	// check block, finalized on light client
	assert_eq!(
		net.lock().peer(3).client().info().finalized_number,
		if FORCE_CHANGE { 0 } else { 10 },
	);
}

#[test]
fn voter_catches_up_to_latest_round_when_behind() {
	let _ = env_logger::try_init();
	let mut runtime = current_thread::Runtime::new().unwrap();
	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	let peers = &[Ed25519Keyring::Alice, Ed25519Keyring::Bob];
	let voters = make_ids(peers);

	let mut net = GrandpaTestNet::new(TestApi::new(voters), 3);
	net.peer(0).push_blocks(50, false);
	net.block_until_sync(&mut runtime);

	let net = Arc::new(Mutex::new(net));
	let mut finality_notifications = Vec::new();

	let voter = |keystore, peer_id, link, net: Arc<Mutex<GrandpaTestNet>>| -> Box<dyn Future<Item=(), Error=()> + Send> {
		let grandpa_params = GrandpaParams {
			config: Config {
				gossip_duration: TEST_GOSSIP_DURATION,
				justification_period: 32,
				keystore,
				name: Some(format!("peer#{}", peer_id)),
				is_authority: true,
				observer_enabled: true,
			},
			link,
			network: net.lock().peer(peer_id).network_service().clone(),
			inherent_data_providers: InherentDataProviders::new(),
			on_exit: Exit,
			telemetry_on_connect: None,
			voting_rule: (),
			executor: threads_pool.clone(),
		};

		Box::new(run_grandpa_voter(grandpa_params).expect("all in order with client and network"))
	};

	let mut keystore_paths = Vec::new();

	// spawn authorities
	for (peer_id, key) in peers.iter().enumerate() {
		let (client, link) = {
			let net = net.lock();
			let link = net.peers[peer_id].data.lock().take().expect("link initialized at startup; qed");
			(
				net.peers[peer_id].client().clone(),
				link,
			)
		};

		finality_notifications.push(
			client.finality_notification_stream()
				.map(|v| Ok::<_, ()>(v)).compat()
				.take_while(|n| Ok(n.header.number() < &50))
				.for_each(move |_| Ok(()))
		);

		let (keystore, keystore_path) = create_keystore(*key);
		keystore_paths.push(keystore_path);

		let voter = voter(Some(keystore), peer_id, link, net.clone());

		runtime.spawn(voter);
	}

	// wait for them to finalize block 50. since they'll vote on 3/4 of the
	// unfinalized chain it will take at least 4 rounds to do it.
	let wait_for_finality = ::futures::future::join_all(finality_notifications)
		.map(|_| ())
		.map_err(|_| ());

	// spawn a new voter, it should be behind by at least 4 rounds and should be
	// able to catch up to the latest round
	let test = {
		let net = net.clone();
		let runtime = runtime.handle();

		wait_for_finality.and_then(move |_| {
			let peer_id = 2;
			let link = {
				let net = net.lock();
				let mut link = net.peers[peer_id].data.lock();
				link.take().expect("link initialized at startup; qed")
			};

			let set_state = link.persistent_data.set_state.clone();

			let voter = voter(None, peer_id, link, net);

			runtime.spawn(voter).unwrap();

			let start_time = std::time::Instant::now();
			let timeout = Duration::from_secs(5 * 60);
			let wait_for_catch_up = futures::future::poll_fn(move || {
				// The voter will start at round 1 and since everyone else is
				// already at a later round the only way to get to round 4 (or
				// later) is by issuing a catch up request.
				if set_state.read().last_completed_round().number >= 4 {
					Ok(Async::Ready(()))
				} else if start_time.elapsed() > timeout {
					panic!("Timed out while waiting for catch up to happen")
				} else {
					Ok(Async::NotReady)
				}
			});

			wait_for_catch_up
		})
	};

	let drive_to_completion = futures::future::poll_fn(|| { net.lock().poll(); Ok(Async::NotReady) });
	let _ = runtime.block_on(test.select(drive_to_completion).map_err(|_| ())).unwrap();
}

#[test]
fn grandpa_environment_respects_voting_rules() {
	use finality_grandpa::Chain;
	use sc_network_test::TestClient;

	let threads_pool = futures03::executor::ThreadPool::new().unwrap();

	let peers = &[Ed25519Keyring::Alice];
	let voters = make_ids(peers);

	let mut net = GrandpaTestNet::new(TestApi::new(voters), 1);
	let peer = net.peer(0);
	let network_service = peer.network_service().clone();
	let link = peer.data.lock().take().unwrap();

	// create a voter environment with a given voting rule
	let environment = |voting_rule: Box<dyn VotingRule<Block, TestClient>>| {
		let PersistentData {
			ref authority_set,
			ref consensus_changes,
			ref set_state,
			..
		} = link.persistent_data;

		let config = Config {
			gossip_duration: TEST_GOSSIP_DURATION,
			justification_period: 32,
			keystore: None,
			name: None,
			is_authority: true,
			observer_enabled: true,
		};

		let network = NetworkBridge::new(
			network_service.clone(),
			config.clone(),
			set_state.clone(),
			&threads_pool,
			Exit,
		);

		Environment {
			authority_set: authority_set.clone(),
			config: config.clone(),
			consensus_changes: consensus_changes.clone(),
			client: link.client.clone(),
			select_chain: link.select_chain.clone(),
			set_id: authority_set.set_id(),
			voter_set_state: set_state.clone(),
			voters: Arc::new(authority_set.current_authorities()),
			network,
			voting_rule,
		}
	};

	// add 21 blocks
	peer.push_blocks(21, false);

	// create an environment with no voting rule restrictions
	let unrestricted_env = environment(Box::new(()));

	// another with 3/4 unfinalized chain voting rule restriction
	let three_quarters_env = environment(Box::new(
		voting_rule::ThreeQuartersOfTheUnfinalizedChain
	));

	// and another restricted with the default voting rules: i.e. 3/4 rule and
	// always below best block
	let default_env = environment(Box::new(
		VotingRulesBuilder::default().build()
	));

	// the unrestricted environment should just return the best block
	assert_eq!(
		unrestricted_env.best_chain_containing(
			peer.client().info().finalized_hash
		).unwrap().1,
		21,
	);

	// both the other environments should return block 16, which is 3/4 of the
	// way in the unfinalized chain
	assert_eq!(
		three_quarters_env.best_chain_containing(
			peer.client().info().finalized_hash
		).unwrap().1,
		16,
	);

	assert_eq!(
		default_env.best_chain_containing(
			peer.client().info().finalized_hash
		).unwrap().1,
		16,
	);

	// we finalize block 19 with block 21 being the best block
	peer.client().finalize_block(BlockId::Number(19), None, false).unwrap();

	// the 3/4 environment should propose block 21 for voting
	assert_eq!(
		three_quarters_env.best_chain_containing(
			peer.client().info().finalized_hash
		).unwrap().1,
		21,
	);

	// while the default environment will always still make sure we don't vote
	// on the best block (2 behind)
	assert_eq!(
		default_env.best_chain_containing(
			peer.client().info().finalized_hash
		).unwrap().1,
		19,
	);

	// we finalize block 21 with block 21 being the best block
	peer.client().finalize_block(BlockId::Number(21), None, false).unwrap();

	// even though the default environment will always try to not vote on the
	// best block, there's a hard rule that we can't cast any votes lower than
	// the given base (#21).
	assert_eq!(
		default_env.best_chain_containing(
			peer.client().info().finalized_hash
		).unwrap().1,
		21,
	);
}

#[test]
fn imports_justification_for_regular_blocks_on_import() {
	// NOTE: this is a regression test since initially we would only import
	// justifications for authority change blocks, and would discard any
	// existing justification otherwise.
	let peers = &[Ed25519Keyring::Alice];
	let voters = make_ids(peers);
	let api = TestApi::new(voters);
	let mut net = GrandpaTestNet::new(api.clone(), 1);

	let client = net.peer(0).client().clone();
	let (mut block_import, ..) = net.make_block_import::<
		TransactionFor<substrate_test_runtime_client::Backend, Block>
	>(client.clone());

	let full_client = client.as_full().expect("only full clients are used in test");
	let builder = full_client.new_block_at(&BlockId::Number(0), Default::default(), false).unwrap();
	let block = builder.build().unwrap().block;

	let block_hash = block.hash();

	// create a valid justification, with one precommit targeting the block
	let justification = {
		let round = 1;
		let set_id = 0;

		let precommit = finality_grandpa::Precommit {
			target_hash: block_hash,
			target_number: *block.header.number(),
		};

		let msg = finality_grandpa::Message::Precommit(precommit.clone());
		let encoded = communication::localized_payload(round, set_id, &msg);
		let signature = peers[0].sign(&encoded[..]).into();

		let precommit = finality_grandpa::SignedPrecommit {
			precommit,
			signature,
			id: peers[0].public().into(),
		};

		let commit = finality_grandpa::Commit {
			target_hash: block_hash,
			target_number: *block.header.number(),
			precommits: vec![precommit],
		};

		GrandpaJustification::from_commit(
			&full_client,
			round,
			commit,
		).unwrap()
	};

	// we import the block with justification attached
	let block = BlockImportParams {
		origin: BlockOrigin::File,
		header: block.header,
		justification: Some(justification.encode()),
		post_digests: Vec::new(),
		body: Some(block.extrinsics),
		storage_changes: None,
		finalized: false,
		auxiliary: Vec::new(),
		fork_choice: ForkChoiceStrategy::LongestChain,
		allow_missing_state: false,
		import_existing: false,
	};

	assert_eq!(
		block_import.import_block(block, HashMap::new()).unwrap(),
		ImportResult::Imported(ImportedAux {
			needs_justification: false,
			clear_justification_requests: false,
			bad_justification: false,
			is_new_best: true,
			..Default::default()
		}),
	);

	// the justification should be imported and available from the client
	assert!(
		client.justification(&BlockId::Hash(block_hash)).unwrap().is_some(),
	);
}
