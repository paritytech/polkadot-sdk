//! End-to-end tests for the node-side price-oracle pipeline.
//!
//! Unlike `inherent_e2e.rs`, these tests exercise the **real node-side code**:
//! `fetch_and_cache` performs a live HTTP fetch against an in-process mock
//! server, and `create_inherent_data` produces the inherent from a populated
//! `NudgeStore`. The produced inherent is then fed into the runtime via the
//! existing `init_polkadot_block_builder_with_nudges` entry point — the
//! runtime sees exactly what a real node would have built.

use std::sync::Arc;

use parking_lot::Mutex;
use polkadot_node_price_oracle::{create_inherent_data, fetch_and_cache, NudgeStore, PriceFetcher};
use polkadot_test_client::{
	construct_extrinsic, BlockBuilderExt, Client, ClientBlockImportExt,
	DefaultTestClientBuilderExt, InitPolkadotBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use sp_api::ProvideRuntimeApi;
use sp_consensus::BlockOrigin;
use sp_consensus_babe::AuthoritySignature;
use sp_consensus_slots::Slot;
use sp_core::crypto::Pair as PairT;
use sp_price_oracle::{Nudge, PriceOracleApi, SignedNudge};
use sp_runtime::FixedU128;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::TcpListener,
};

// ---------- Mock HTTP server ----------

/// A minimal HTTP/1.1 server bound to an ephemeral localhost port.
///
/// Each test gets its own instance; the listener task is aborted on drop, so
/// tests are fully isolated. Responses are keyed by request path — call
/// [`MockServer::add_response`] to register a body for a given path.
struct MockServer {
	addr: std::net::SocketAddr,
	handle: tokio::task::JoinHandle<()>,
	state: Arc<Mutex<MockState>>,
}

#[derive(Default)]
struct MockState {
	/// path → (status_code, body_bytes)
	routes: std::collections::HashMap<String, (u16, Vec<u8>)>,
	/// path → request count
	hits: std::collections::HashMap<String, u32>,
}

impl MockServer {
	async fn start() -> Self {
		let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock server");
		let addr = listener.local_addr().expect("local addr");
		let state: Arc<Mutex<MockState>> = Arc::new(Mutex::new(MockState::default()));
		let state_bg = state.clone();
		let handle = tokio::spawn(async move {
			loop {
				let Ok((mut sock, _)) = listener.accept().await else { return };
				let state = state_bg.clone();
				tokio::spawn(async move {
					// Read a single request (test bodies are small).
					let mut buf = [0u8; 4096];
					let n = match sock.read(&mut buf).await {
						Ok(n) if n > 0 => n,
						_ => return,
					};
					let req = String::from_utf8_lossy(&buf[..n]);
					let path = req
						.lines()
						.next()
						.and_then(|l| l.split_whitespace().nth(1))
						.unwrap_or("/")
						.to_string();

					let (status, body) = {
						let mut st = state.lock();
						*st.hits.entry(path.clone()).or_insert(0) += 1;
						st.routes
							.get(&path)
							.cloned()
							.unwrap_or((404, b"not found".to_vec()))
					};
					let status_line = match status {
						200 => "200 OK",
						500 => "500 Internal Server Error",
						_ => "200 OK",
					};
					let head = format!(
						"HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
						status_line,
						body.len()
					);
					let _ = sock.write_all(head.as_bytes()).await;
					let _ = sock.write_all(&body).await;
					let _ = sock.shutdown().await;
				});
			}
		});
		Self { addr, handle, state }
	}

	fn add_response(&self, path: &str, status: u16, body: impl Into<Vec<u8>>) {
		self.state.lock().routes.insert(path.to_string(), (status, body.into()));
	}

	fn url(&self, path: &str) -> String {
		format!("http://{}{}", self.addr, path)
	}

	fn hits(&self, path: &str) -> u32 {
		*self.state.lock().hits.get(path).unwrap_or(&0)
	}
}

impl Drop for MockServer {
	fn drop(&mut self) {
		self.handle.abort();
	}
}

// ---------- Signing helpers ----------

fn alice_babe_pair() -> sp_core::sr25519::Pair {
	sp_core::sr25519::Pair::from_string("//Alice", None).expect("valid seed")
}

fn bob_babe_pair() -> sp_core::sr25519::Pair {
	sp_core::sr25519::Pair::from_string("//Bob", None).expect("valid seed")
}

fn make_signed_nudge(
	pair: &sp_core::sr25519::Pair,
	nudge: Nudge,
	slot: u64,
	authority_index: u32,
) -> SignedNudge {
	let slot = Slot::from(slot);
	let payload = SignedNudge::signing_payload(&nudge, slot);
	let sig = pair.sign(&payload);
	SignedNudge { nudge, slot, authority_index, signature: AuthoritySignature::from(sig) }
}

fn current_slot_ms() -> u64 {
	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	now_ms / 6000
}

/// Build + import a block that points the runtime's endpoint list at `urls`
/// via the root-only `set_active_endpoints` extrinsic. `urls` carries
/// `(parsing_method_id, url)` pairs — `0` is the Binance decoder.
///
/// The test runtime's `MinNudges = 1`, so the oracle inherent must carry at
/// least one valid nudge for the block to be accepted — we include one
/// signed by Alice targeting the current slot.
async fn seed_endpoints(client: &Client, urls: Vec<(u8, String)>) {
	use polkadot_test_client::runtime::RuntimeCall;

	let inner = RuntimeCall::PriceOracle(pallet_price_oracle::Call::set_active_endpoints {
		endpoints: urls.into_iter().map(|(id, u)| (id, u.into_bytes())).collect(),
	});
	let sudo = RuntimeCall::Sudo(pallet_sudo::Call::sudo { call: Box::new(inner) });
	let ext = construct_extrinsic(client, sudo, sp_keyring::Sr25519Keyring::Alice, 0);

	let slot = current_slot_ms();
	let nudge = make_signed_nudge(&alice_babe_pair(), Nudge::Up, slot, 0);
	let mut block_builder = client.init_polkadot_block_builder_with_nudges(vec![nudge]);
	block_builder.push_polkadot_extrinsic(ext).expect("push sudo extrinsic");
	let block = block_builder.build().expect("build block").block;
	client.import(BlockOrigin::Own, block).await.expect("import endpoint-seed block");
}

/// Build + import a block whose oracle inherent is produced by the real
/// node-side `create_inherent_data` against the provided `NudgeStore`.
async fn build_and_import_with_store(client: &Client, store: &NudgeStore) {
	let best = client.chain_info().best_hash;
	let nudges = create_inherent_data::<polkadot_primitives::Block, _>(client, store, best);
	let block = client
		.init_polkadot_block_builder_with_nudges(nudges)
		.build()
		.expect("builds block from real inherent")
		.block;
	client.import(BlockOrigin::Own, block).await.expect("imports block from real inherent");
}

// ---------- Tests ----------

/// `fetch_and_cache` hits the configured endpoint, decodes a Binance
/// response, and writes the result into `NudgeStore::cached_price`.
#[tokio::test(flavor = "multi_thread")]
async fn fetch_and_cache_populates_cached_price() {
	let server = MockServer::start().await;
	server.add_response("/dot", 200, br#"{"symbol":"DOTUSDT","price":"7.50000000"}"#.to_vec());

	let client = TestClientBuilder::new().build();
	seed_endpoints(&client, vec![(0, server.url("/dot"))]).await;

	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;

	assert_eq!(
		store.cached_price(),
		Some(FixedU128::from_rational(75, 10)),
		"cached price should equal the decoded Binance value"
	);
	assert_eq!(server.hits("/dot"), 1, "fetcher must have hit the endpoint once");
}

/// If the primary endpoint fails at the network layer, the fetcher falls
/// back to another one and still populates the cache.
///
/// We build the "broken" URL by binding a TCP port, grabbing its address,
/// then dropping the listener — requests to that address get a connection
/// refused, which `reqwest` reports as an error (unlike a 500 response,
/// which `fetch_raw` would consider a successful fetch).
#[tokio::test(flavor = "multi_thread")]
async fn fetch_and_cache_falls_back_on_primary_failure() {
	let server = MockServer::start().await;
	server.add_response("/ok", 200, br#"{"symbol":"DOTUSDT","price":"4.20000000"}"#.to_vec());

	let broken_url = {
		let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind broken port");
		let addr = listener.local_addr().expect("broken addr");
		drop(listener);
		format!("http://{}/unreachable", addr)
	};

	let client = TestClientBuilder::new().build();
	seed_endpoints(&client, vec![(0, broken_url), (0, server.url("/ok"))]).await;

	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;

	assert_eq!(store.cached_price(), Some(FixedU128::from_rational(42, 10)));
	assert!(server.hits("/ok") >= 1, "fallback endpoint should have been hit");
}

/// With every endpoint failing, the cache is left untouched.
#[tokio::test(flavor = "multi_thread")]
async fn fetch_and_cache_noop_when_all_endpoints_fail() {
	let server = MockServer::start().await;
	server.add_response("/a", 500, b"boom".to_vec());
	server.add_response("/b", 500, b"boom".to_vec());

	let client = TestClientBuilder::new().build();
	seed_endpoints(&client, vec![(0, server.url("/a")), (0, server.url("/b"))]).await;

	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;

	assert!(store.cached_price().is_none(), "cache must stay empty when all endpoints fail");
}

/// Full pipeline: fetcher populates the store, a gossiped foreign nudge
/// lands in the store, `create_inherent_data` picks the matching-direction
/// nudge, block builds, runtime applies the price bump.
#[tokio::test(flavor = "multi_thread")]
async fn full_pipeline_fetch_then_build_block() {
	let server = MockServer::start().await;
	// cached > onchain (onchain starts at 0) → direction = Up.
	server.add_response("/dot", 200, br#"{"symbol":"DOTUSDT","price":"7.50000000"}"#.to_vec());

	let client = TestClientBuilder::new().build();
	seed_endpoints(&client, vec![(0, server.url("/dot"))]).await;

	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;
	assert!(store.cached_price().is_some());

	// Gossiped Bob nudge in the correct direction — Alice's slot need not
	// match because we do not enter produce_and_sign_nudge here.
	let slot = current_slot_ms();
	store.insert(make_signed_nudge(&bob_babe_pair(), Nudge::Up, slot, 1));

	let price_before = client
		.runtime_api()
		.current_price(client.chain_info().best_hash)
		.expect("current_price");
	build_and_import_with_store(&client, &store).await;
	let price_after = client
		.runtime_api()
		.current_price(client.chain_info().best_hash)
		.expect("current_price after");

	assert_eq!(
		price_after,
		price_before + FixedU128::from_rational(1, 100),
		"single Up nudge should move the price by one epsilon"
	);
}

/// When the store holds only wrong-direction nudges, `create_inherent_data`
/// picks nothing.
#[tokio::test(flavor = "multi_thread")]
async fn wrong_direction_nudges_not_selected() {
	let server = MockServer::start().await;
	// cached_price > onchain → direction = Up. Store holds only Down.
	server.add_response("/dot", 200, br#"{"symbol":"DOTUSDT","price":"3.00000000"}"#.to_vec());

	let client = TestClientBuilder::new().build();
	seed_endpoints(&client, vec![(0, server.url("/dot"))]).await;

	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;

	let slot = current_slot_ms();
	store.insert(make_signed_nudge(&alice_babe_pair(), Nudge::Down, slot, 0));
	store.insert(make_signed_nudge(&bob_babe_pair(), Nudge::Down, slot, 1));

	let best = client.chain_info().best_hash;
	let produced = create_inherent_data::<polkadot_primitives::Block, _>(&client, &store, best);
	assert!(produced.is_empty(), "no nudges should be selected when all are wrong-direction");
}

/// Stale foreign nudges (slot outside the validity window) are filtered by
/// the store before `create_inherent_data` considers them, so the produced
/// inherent is empty even when the cached price calls for a nudge.
#[tokio::test(flavor = "multi_thread")]
async fn stale_foreign_nudges_filtered_out() {
	let server = MockServer::start().await;
	server.add_response("/dot", 200, br#"{"symbol":"DOTUSDT","price":"5.00000000"}"#.to_vec());

	let client = TestClientBuilder::new().build();
	seed_endpoints(&client, vec![(0, server.url("/dot"))]).await;

	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;

	// Test runtime's NudgeValidity is 10 slots — put one nudge well before
	// the window and nothing else.
	let slot = current_slot_ms();
	let stale = slot.saturating_sub(100);
	store.insert(make_signed_nudge(&bob_babe_pair(), Nudge::Up, stale, 1));

	let best = client.chain_info().best_hash;
	let produced = create_inherent_data::<polkadot_primitives::Block, _>(&client, &store, best);
	assert!(produced.is_empty(), "stale nudges must not appear in the inherent");
}

/// `create_inherent_data` caps the selection at `needed = diff / epsilon`,
/// even if the store holds more matching-direction nudges than that.
///
/// After `seed_endpoints` the on-chain price is 0.01 (one Up nudge). A
/// cached price of 0.02 gives `diff = 0.01 = 1*epsilon`, so `needed = 1`
/// even though the store has two Up nudges available.
#[tokio::test(flavor = "multi_thread")]
async fn inherent_respects_needed_cap() {
	let server = MockServer::start().await;
	server.add_response("/dot", 200, br#"{"symbol":"DOTUSDT","price":"0.02000000"}"#.to_vec());

	let client = TestClientBuilder::new().build();
	seed_endpoints(&client, vec![(0, server.url("/dot"))]).await;

	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;

	let slot = current_slot_ms();
	store.insert(make_signed_nudge(&alice_babe_pair(), Nudge::Up, slot, 0));
	store.insert(make_signed_nudge(&bob_babe_pair(), Nudge::Up, slot, 1));

	let best = client.chain_info().best_hash;
	let produced = create_inherent_data::<polkadot_primitives::Block, _>(&client, &store, best);
	assert_eq!(produced.len(), 1, "only `needed=1` nudge should be selected");

	let price_before = client.runtime_api().current_price(best).expect("price before");
	build_and_import_with_store(&client, &store).await;
	let price_after = client
		.runtime_api()
		.current_price(client.chain_info().best_hash)
		.expect("price after");
	assert_eq!(
		price_after,
		price_before + FixedU128::from_rational(1, 100),
		"price must move by exactly one epsilon despite extra nudges in store"
	);
}

/// Without a cached price the inherent is empty — no endpoints seeded means
/// `fetch_and_cache` cannot populate the store, and `create_inherent_data`
/// returns an empty vec.
#[tokio::test(flavor = "multi_thread")]
async fn no_cached_price_produces_empty_inherent() {
	let client = TestClientBuilder::new().build();
	// No endpoints seeded — fetch_and_cache will warn and return.
	let store = NudgeStore::new();
	let fetcher = PriceFetcher::new();
	fetch_and_cache::<polkadot_primitives::Block, _>(&client, &fetcher, &store).await;
	assert!(store.cached_price().is_none());

	// Even if a foreign nudge is present, without a cached price there is
	// no target for the inherent to drive toward.
	let slot = current_slot_ms();
	store.insert(make_signed_nudge(&bob_babe_pair(), Nudge::Up, slot, 1));

	let best = client.chain_info().best_hash;
	let produced = create_inherent_data::<polkadot_primitives::Block, _>(&client, &store, best);
	assert!(produced.is_empty(), "inherent must be empty without a cached price");
}
