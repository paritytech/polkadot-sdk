use crate::client::Client;
use crate::config::SetConfig;
use futures::channel::oneshot;
use futures::Future;
use futures::future::BoxFuture;
use futures::Stream;
use prometheus_endpoint::Registry;
use prost::Message;
use sc_chain_spec::BuildGenesisBlock;
use sc_client_api::Backend;
use sc_client_api::BlockBackend;
use sc_client_api::BlockImportOperation;
use sc_client_api::CallExecutor;
use sc_client_api::ChildInfo;
use sc_client_api::execution_extensions::ExecutionExtensions;
use sc_client_api::in_mem::Backend as InMemoryBackend;
use sc_client_api::NewBlockState;
use sc_client_api::ProofProvider;
use sc_executor::RuntimeVersion;
use sc_executor::RuntimeVersionOf;
use sc_network_common::ExHashT;
use sc_network_sync::state_request_handler::StateRequestHandler;
use sc_network_sync::state_request_handler::StateSyncProtocolNames;
use sc_network_sync::StateResponse;
use sc_network_sync::strategy::state_sync::ImportResult;
use sc_network_sync::strategy::state_sync::StateSync;
use sc_network_sync::strategy::state_sync::StateSyncProvider;
use sc_network_types::kad::Record;
use sc_network::config::MultiaddrWithPeerId;
use sc_network::config::NotificationHandshake;
use sc_network::config::ProtocolId;
use sc_network::IfDisconnected;
use sc_network::KademliaKey;
use sc_network::Multiaddr;
use sc_network::network_state::NetworkState;
use sc_network::NetworkBackend;
use sc_network::NetworkDHTProvider;
use sc_network::NetworkEventStream;
use sc_network::NetworkPeers;
use sc_network::NetworkRequest;
use sc_network::NetworkSigner;
use sc_network::NetworkStateInfo;
use sc_network::NetworkStatus;
use sc_network::NetworkStatusProvider;
use sc_network::NotificationConfig;
use sc_network::NotificationMetrics;
use sc_network::NotificationService;
use sc_network::ObservedRole;
use sc_network::peer_store::PeerStoreProvider;
use sc_network::PeerId;
use sc_network::ProtocolName;
use sc_network::ReputationChange;
use sc_network::request_responses::IncomingRequest;
use sc_network::RequestFailure;
use sc_network::service::traits::NetworkService;
use sc_network::service::traits::PeerStore;
use sc_network::service::traits::RequestResponseConfig;
use sc_network::service::traits::SigningError;
use sc_network::Signature;
use sp_api::ProofRecorder;
use sp_core::traits::CallContext;
use sp_core::traits::Externalities;
use sp_core::traits::SpawnNamed;
use sp_externalities::Extensions;
use sp_runtime::traits::Block as BlockT;
use sp_runtime::traits::HashingFor;
use sp_state_machine::KeyValueStates;
use sp_state_machine::KeyValueStorageLevel;
use sp_state_machine::LayoutV1;
use sp_state_machine::OverlayedChanges;
use sp_trie::ClientProof;
use sp_trie::CompactProof;
use sp_trie::PrefixedMemoryDB;
use sp_trie::StorageProof;
use sp_trie::Trie;
use sp_trie::TrieDBBuilder;
use sp_trie::TrieDBMutBuilder;
use sp_trie::TrieMut;
use std::cell::RefCell;
use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use substrate_test_runtime::Block;
use substrate_test_runtime::Header;

// Using `mockall` caused `'lifetime` compile error
struct MockClientApi;
#[allow(unused)]
impl<B: BlockT> ProofProvider<B> for MockClientApi {
  fn read_proof(&self, hash: B::Hash, keys: &mut dyn Iterator<Item = &[u8]>) -> sp_blockchain::Result<StorageProof> { todo!() }
  fn read_child_proof(&self, hash: B::Hash, child_info: &ChildInfo, keys: &mut dyn Iterator<Item = &[u8]>) -> sp_blockchain::Result<StorageProof> { todo!() }
  fn execution_proof(&self, hash: B::Hash, method: &str, call_data: &[u8]) -> sp_blockchain::Result<(Vec<u8>, StorageProof)> { todo!() }
  fn read_proof_collection(&self, hash: B::Hash, start_keys: &[Vec<u8>], size_limit: usize) -> sp_blockchain::Result<(CompactProof, u32)> { todo!() }
  fn storage_collection(&self, hash: B::Hash, start_key: &[Vec<u8>], size_limit: usize) -> sp_blockchain::Result<Vec<(KeyValueStorageLevel, bool)>> { todo!() }
  fn verify_range_proof(&self, root: B::Hash, proof: CompactProof, start_keys: &[Vec<u8>]) -> sp_blockchain::Result<(KeyValueStates, usize)> { todo!() }
  fn proposal_prove(&self, client_proof: &ClientProof<B::Hash>, size_limit: usize) -> sp_blockchain::Result<Vec<CompactProof>> { todo!() }
}

#[derive(Clone)]
struct MockSpawnNamed;
#[allow(unused)]
impl SpawnNamed for MockSpawnNamed {
  fn spawn_blocking(&self, name: &'static str, group: Option<&'static str>, future: futures::future::BoxFuture<'static, ()>) {}
  fn spawn(&self, name: &'static str, group: Option<&'static str>, future: futures::future::BoxFuture<'static, ()>) {}
}

#[derive(Clone)]
struct MockCallExecutor;
#[allow(unused)]
impl<B: BlockT> CallExecutor<B> for MockCallExecutor {
  type Error = String;
	type Backend = InMemoryBackend<B>;
	fn execution_extensions(&self) -> &ExecutionExtensions<B> { todo!() }
	fn call(&self, at_hash: B::Hash, method: &str, call_data: &[u8], context: CallContext) -> Result<Vec<u8>, sp_blockchain::Error> { todo!() }
	fn contextual_call(&self, at_hash: B::Hash, method: &str, call_data: &[u8], changes: &RefCell<OverlayedChanges<HashingFor<B>>>, proof_recorder: &Option<ProofRecorder<B>>, call_context: CallContext, extensions: &RefCell<Extensions>) -> sp_blockchain::Result<Vec<u8>> { todo!() }
	fn runtime_version(&self, at_hash: B::Hash) -> Result<RuntimeVersion, sp_blockchain::Error> { todo!() }
	fn prove_execution(&self, at_hash: B::Hash, method: &str, call_data: &[u8]) -> Result<(Vec<u8>, StorageProof), sp_blockchain::Error> { todo!() }
}
#[allow(unused)]
impl RuntimeVersionOf for MockCallExecutor {
  fn runtime_version(&self, ext: &mut dyn Externalities, runtime_code: &sp_core::traits::RuntimeCode) -> sc_executor_common::error::Result<RuntimeVersion> { todo!() }
}

struct MockBuildGenesisBlock<B: BlockT>((B, <InMemoryBackend<B> as Backend<B>>::BlockImportOperation));
#[allow(unused)]
impl<B: BlockT> BuildGenesisBlock<B> for MockBuildGenesisBlock<B> {
	type BlockImportOperation = <InMemoryBackend<B> as Backend<B>>::BlockImportOperation;
	fn build_genesis_block(self) -> sp_blockchain::Result<(B, Self::BlockImportOperation)> {
    Ok(self.0)
  }
}

#[derive(Debug)]
struct MockPeerStore;
#[async_trait::async_trait]
impl PeerStore for MockPeerStore {
	fn handle(&self) -> Arc<dyn PeerStoreProvider> { todo!() }
  async fn run(self) { todo!() }
}

#[derive(Clone)]
struct MockNetworkService;
#[allow(unused)]
impl NetworkSigner for MockNetworkService {
	fn sign_with_local_identity(&self, msg: Vec<u8>) -> Result<Signature, SigningError> { todo!() }
	fn verify(&self, peer_id: sc_network_types::PeerId, public_key: &Vec<u8>, signature: &Vec<u8>, message: &Vec<u8>) -> Result<bool, String> { todo!() }
}
#[allow(unused)]
impl NetworkDHTProvider for MockNetworkService {
	fn find_closest_peers(&self, target: PeerId) { todo!() }
	fn get_value(&self, key: &KademliaKey) { todo!() }
	fn put_value(&self, key: KademliaKey, value: Vec<u8>) { todo!() }
	fn put_record_to(&self, record: Record, peers: HashSet<PeerId>, update_local_storage: bool) { todo!() }
	fn store_record(&self, key: KademliaKey, value: Vec<u8>, publisher: Option<PeerId>, expires: Option<Instant>) { todo!() }
	fn start_providing(&self, key: KademliaKey) { todo!() }
	fn stop_providing(&self, key: KademliaKey) { todo!() }
	fn get_providers(&self, key: KademliaKey) { todo!() }
}
#[allow(unused)]
#[async_trait::async_trait]
impl NetworkStatusProvider for MockNetworkService {
  async fn status(&self) -> Result<NetworkStatus, ()> { todo!() }
  async fn network_state(&self) -> Result<NetworkState, ()> { todo!() }
}
#[allow(unused)]
#[async_trait::async_trait]
impl NetworkPeers for MockNetworkService {
  fn set_authorized_peers(&self, peers: HashSet<PeerId>) { todo!() }
	fn set_authorized_only(&self, reserved_only: bool) { todo!() }
	fn add_known_address(&self, peer_id: PeerId, addr: Multiaddr) { todo!() }
	fn report_peer(&self, peer_id: PeerId, cost_benefit: ReputationChange) { todo!() }
	fn peer_reputation(&self, peer_id: &PeerId) -> i32 { todo!() }
	fn disconnect_peer(&self, peer_id: PeerId, protocol: ProtocolName) { todo!() }
	fn accept_unreserved_peers(&self) { todo!() }
	fn deny_unreserved_peers(&self) { todo!() }
	fn add_reserved_peer(&self, peer: MultiaddrWithPeerId) -> Result<(), String> { todo!() }
	fn remove_reserved_peer(&self, peer_id: PeerId) { todo!() }
  fn set_reserved_peers(&self, protocol: ProtocolName, peers: HashSet<Multiaddr>) -> Result<(), String> { todo!() }
  fn add_peers_to_reserved_set(&self, protocol: ProtocolName, peers: HashSet<Multiaddr>) -> Result<(), String> { todo!() }
  fn remove_peers_from_reserved_set(&self, protocol: ProtocolName, peers: Vec<PeerId>) -> Result<(), String> { todo!() }
  fn sync_num_connected(&self) -> usize { todo!() }
  fn peer_role(&self, peer_id: PeerId, handshake: Vec<u8>) -> Option<ObservedRole> { todo!() }
  async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()> { todo!() }
}
#[allow(unused)]
impl NetworkEventStream for MockNetworkService {
  fn event_stream(&self, name: &'static str) -> Pin<Box<dyn Stream<Item = sc_network::event::Event> + Send>> { todo!() }
}
#[allow(unused)]
impl NetworkStateInfo for MockNetworkService {
  fn external_addresses(&self) -> Vec<Multiaddr> { todo!() }
	fn listen_addresses(&self) -> Vec<Multiaddr> { todo!() }
	fn local_peer_id(&self) -> PeerId { todo!() }
}
#[allow(unused)]
#[async_trait::async_trait]
impl NetworkRequest for MockNetworkService {
	async fn request(&self, target: PeerId, protocol: ProtocolName, request: Vec<u8>, fallback_request: Option<(Vec<u8>, ProtocolName)>, connect: IfDisconnected) -> Result<(Vec<u8>, ProtocolName), RequestFailure> { todo!() }
	fn start_request(&self, target: PeerId, protocol: ProtocolName, request: Vec<u8>, fallback_request: Option<(Vec<u8>, ProtocolName)>, tx: oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>, connect: IfDisconnected) { todo!() }
}

#[derive(Debug)]
struct MockNotificationConfig;
impl NotificationConfig for MockNotificationConfig {
	fn set_config(&self) -> &SetConfig { todo!() }
	fn protocol_name(&self) -> &ProtocolName { todo!() }
}

#[derive(Debug)]
struct MockRequestResponseConfig(Option<async_channel::Sender<IncomingRequest>>);
impl RequestResponseConfig for MockRequestResponseConfig {
	fn protocol_name(&self) -> &ProtocolName { todo!() }
}

struct MockNetworkBackend;
#[allow(unused)]
#[async_trait::async_trait]
impl<B: BlockT, H: ExHashT> NetworkBackend<B, H> for MockNetworkBackend {
  type NotificationProtocolConfig = MockNotificationConfig;
  type RequestResponseProtocolConfig = MockRequestResponseConfig;
  type NetworkService<Block, Hash> = MockNetworkService;
  type PeerStore = MockPeerStore;
  type BitswapConfig = MockNetworkService;
	fn new(params: sc_network::config::Params<B, H, Self>) -> Result<Self, sc_network::error::Error> where Self: Sized { todo!() }
	fn network_service(&self) -> Arc<dyn NetworkService> { todo!() }
	fn peer_store(bootnodes: Vec<PeerId>, metrics_registry: Option<Registry>) -> Self::PeerStore { todo!() }
	fn register_notification_metrics(registry: Option<&Registry>) -> NotificationMetrics { todo!() }
	fn bitswap_server(client: Arc<dyn BlockBackend<B> + Send + Sync>) -> (Pin<Box<dyn Future<Output = ()> + Send>>, Self::BitswapConfig) { todo!() }
	fn notification_config(protocol_name: ProtocolName, fallback_names: Vec<ProtocolName>, max_notification_size: u64, handshake: Option<NotificationHandshake>, set_config: SetConfig, metrics: NotificationMetrics, peerstore_handle: Arc<dyn PeerStoreProvider>) -> (Self::NotificationProtocolConfig, Box<dyn NotificationService>) { todo!() }
	fn request_response_config(protocol_name: ProtocolName, fallback_names: Vec<ProtocolName>, max_request_size: u64, max_response_size: u64, request_timeout: Duration, inbound_queue: Option<async_channel::Sender<IncomingRequest>>) -> Self::RequestResponseProtocolConfig {
    MockRequestResponseConfig(inbound_queue)
  }
	async fn run(mut self) { todo!() }
}

#[derive(Clone)]
struct Data {
  key_values: Vec<(Vec<u8>, Vec<u8>)>,
  db: PrefixedMemoryDB<HashingFor<Block>>,
  header: Header,
}
impl Data {
  fn new() -> Self {
    let key_values = vec![
      (vec![0u8; 40], vec![0u8; 1]),
      (vec![1u8; 40], vec![1u8; 1]),
    ];
    let mut db = PrefixedMemoryDB::default();
    let mut state_root: <Block as BlockT>::Hash = Default::default();
    let mut trie = TrieDBMutBuilder::<LayoutV1<HashingFor<Block>>>::new(&mut db, &mut state_root).build();
    for (k, v) in &key_values {
      trie.insert(k, v).unwrap();
    }
    trie.commit();
    drop(trie);
    let header = Header {
      number: 1,
      parent_hash: Default::default(),
      state_root,
      digest: Default::default(),
      extrinsics_root: Default::default(),
    };
    Data {
      key_values,
      db,
      header,
    }
  }
}

fn make_server(data: Data) -> (StateSyncProtocolNames, impl FnMut(Vec<u8>) -> BoxFuture<'static, StateResponse>) {
  let backend = Arc::new(InMemoryBackend::<Block>::new());
  backend.import_partial_state(data.db.clone()).unwrap();
  let mut op = backend.begin_operation().unwrap();
  op.set_block_data(data.header.clone(), None, None, None, NewBlockState::Normal).unwrap();
  op.commit_complete_partial_state();
  backend.commit_operation(op).unwrap();
  let client = Arc::new(Client::<_, _, _, ()>::new(
    backend.clone(),
    MockCallExecutor,
    Box::new(MockSpawnNamed),
    MockBuildGenesisBlock((Block {
      header: Header {
        number: 0,
        parent_hash: Default::default(),
        state_root: Default::default(),
        digest: Default::default(),
        extrinsics_root: Default::default(),
      },
      extrinsics: vec![]
    }, backend.begin_operation().unwrap())),
    Default::default(),
    Default::default(),
    None,
    None,
    Default::default(),
  ).unwrap());
  let (handler, protocol_config, protocol_names) = StateRequestHandler::new::<MockNetworkBackend>(&ProtocolId::from("test"), None, client, 0);
  let inbound_queue = protocol_config[0].0.clone().unwrap();
  tokio::spawn(async move {
    handler.run().await;
  });
  let server = move |payload| {
    let inbound_queue = inbound_queue.clone();
    Box::pin(async move {
      let peer = PeerId::random();
      let (pending_response, rx) = oneshot::channel();
      inbound_queue.send(IncomingRequest { peer, payload, pending_response }).await.unwrap();
      let response = rx.await.unwrap().result.unwrap();
      StateResponse::decode(&response[..]).unwrap()
    }) as BoxFuture<'static, StateResponse>
  };
  (protocol_names, server)
}

async fn state_sync(mut use_request_v3: impl FnMut() -> bool) {
  let data = Data::new();
  let (protocol_names, mut server) = make_server(data.clone());
  let mut client = StateSync::<Block, _>::new(Arc::new(MockClientApi), data.header.clone(), None, None, false);
  let mut db = PrefixedMemoryDB::default();
  loop {
    let request = client.next_request();
    let (_, request_v3, request_v2) = protocol_names.encode_request(&request);
    let request_v2 = request_v2.unwrap().0;
    let response = server(if use_request_v3() { request_v3 } else { request_v2 }).await;
    let mut result = client.import(response);
    if let Some(partial_state) = result.take_partial_state() {
      db.consolidate(partial_state);
    }
    match result {
      ImportResult::Import { .. } => {
        break;
      },
      ImportResult::Continue { .. } => {},
      ImportResult::BadResponse => panic!(),
    }
  }
  let trie = TrieDBBuilder::<LayoutV1<HashingFor<Block>>>::new(&db, &data.header.state_root).build();
  let key_values: Vec<_> = trie.iter().unwrap()
      .map(Result::unwrap)
			.collect();
  assert_eq!(key_values, data.key_values);
}

#[tokio::test]
async fn state_sync_v2() {
  state_sync(|| false).await;
}

#[tokio::test]
async fn state_sync_v3() {
  state_sync(|| true).await;
}

#[tokio::test]
async fn state_sync_v2_v3() {
  let mut use_request_v3 = false;
  state_sync(|| {
    use_request_v3 = !use_request_v3;
    use_request_v3
  }).await;
}
