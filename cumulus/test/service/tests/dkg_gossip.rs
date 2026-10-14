//! In-process multi-node test for the DKG gadget.
//!
//! Spins up relay validators (Alice, Bob) and two parachain collators
//! (Charlie, Dave — deliberately different keyring identities than the relay
//! validators, same pattern as the transaction-throughput bench) and asserts
//! that the collators' DKG workers deal once per session and collect each
//! other's dealings over gossip, across a session boundary.
//!
//! Only compiled with `with-authority-discovery`: that runtime variant
//! registers the BEEFY session key the DKG reuses, and the service wires the
//! gossip protocol, the derived BEEFY keystore key, and the worker task for
//! collators. Without the feature this test target is empty.
//!
//! Run with:
//! `cargo test -p cumulus-test-service --features with-authority-discovery --test dkg_gossip`
#![cfg(feature = "with-authority-discovery")]

use std::{sync::atomic::Ordering, time::Duration};

use adkg_vrf::dkg::transcript::Transcript;
use ark_bls12_381::Bls12_381;
use ark_serialize::CanonicalDeserialize;
use codec::Decode;
use cumulus_client_cli::get_raw_genesis_header;
use cumulus_primitives_core::ParaId;
use cumulus_test_service::{
	Keyring::{Alice, Bob, Charlie, Dave},
	TestNodeBuilder, run_relay_chain_validator_node,
};
use polkadot_primitives::HeadData;
use sc_client_api::StorageProvider;
use sc_dkg::CollectionStatsHandle;
use sp_core::storage::StorageKey;
use sp_crypto_hashing::twox_128;

const PARA_ID: u32 = 100;

/// Poll `stats` until the node collected at least `min_dealings` dealings and
/// saw a dealing for at least session `min_session`.
async fn wait_for_collection(
	node: &str,
	stats: &CollectionStatsHandle,
	min_dealings: u64,
	min_session: u32,
) -> (u64, u32) {
	tokio::time::timeout(Duration::from_secs(600), async {
		loop {
			let dealings = stats.dealings.load(Ordering::Relaxed);
			let latest_session = stats.latest_session.load(Ordering::Relaxed);
			if dealings >= min_dealings && latest_session >= min_session {
				return (dealings, latest_session);
			}
			tokio::time::sleep(Duration::from_secs(5)).await;
		}
	})
	.await
	.unwrap_or_else(|_| {
		panic!(
			"{node}: timed out waiting for >= {min_dealings} dealings in session >= {min_session} \
			 (have {} dealings, latest session {})",
			stats.dealings.load(Ordering::Relaxed),
			stats.latest_session.load(Ordering::Relaxed),
		)
	})
}

/// Poll `KeyGen::AggregatedTranscript` until it holds an aggregate of at
/// least `min_dealers` single-dealer transcripts.
async fn wait_for_aggregated_transcript(
	client: &cumulus_test_service::Client,
	min_dealers: usize,
) -> usize {
	let key = StorageKey(
		[twox_128(b"KeyGen"), twox_128(b"AggregatedTranscript")].concat(),
	);
	tokio::time::timeout(Duration::from_secs(300), async {
		loop {
			let best_hash = client.chain_info().best_hash;
			let dealers = client
				.storage(best_hash, &key)
				.ok()
				.flatten()
				.and_then(|data| Vec::<u8>::decode(&mut &data.0[..]).ok())
				.and_then(|bytes| {
					Transcript::<Bls12_381>::deserialize_compressed(&bytes[..]).ok()
				})
				.map(|t| t.list_dealers().len())
				.unwrap_or(0);
			if dealers >= min_dealers {
				return dealers
			}
			tokio::time::sleep(Duration::from_secs(2)).await;
		}
	})
	.await
	.expect("timed out waiting for the aggregated transcript on-chain")
}

#[test]
fn dkg_dealings_gossiped_and_collected_per_session() {
	sp_tracing::try_init_simple();
	let mut builder = sc_cli::LoggerBuilder::new("");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(PARA_ID);
	// `run_relay_chain_validator_node` does `handle.block_on` internally, so
	// it must be called from outside the runtime (same pattern as the
	// transaction-throughput bench).
	let runtime = tokio::runtime::Runtime::new().expect("Creates tokio runtime");
	let tokio_handle = runtime.handle().clone();

	// Relay validators.
	let alice = run_relay_chain_validator_node(tokio_handle.clone(), Alice, || {}, vec![], None);
	let bob = run_relay_chain_validator_node(
		tokio_handle.clone(),
		Bob,
		|| {},
		vec![alice.addr.clone()],
		None,
	);

	// First collator.
	let charlie = runtime.block_on(
		TestNodeBuilder::new(para_id, tokio_handle.clone(), Charlie)
			.enable_collator()
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build(),
	);

	// Register the parachain so collators can produce blocks.
	runtime
		.block_on(alice.register_parachain(
			para_id,
			cumulus_test_service::runtime::WASM_BINARY
				.expect("You need to build the WASM binary to run this test!")
				.to_vec(),
			HeadData(
				get_raw_genesis_header(charlie.client.clone())
					.expect("Unable to get genesis HeadData."),
			),
		))
		.unwrap();

	// Second collator, peered with the first on the parachain network (the
	// DKG gossip protocol rides on the same network).
	let dave = runtime.block_on(
		TestNodeBuilder::new(para_id, tokio_handle.clone(), Dave)
			.enable_collator()
			.connect_to_parachain_node(&charlie)
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build(),
	);

	runtime.block_on(dave.wait_for_blocks(1));

	let charlie_stats = charlie.dkg_collection_stats.clone().expect("collator runs the DKG worker");
	let dave_stats = dave.dkg_collection_stats.clone().expect("collator runs the DKG worker");

	runtime.block_on(async {
		// Phase 1: both collators deal in the current session and collect the
		// other's dealing over gossip.
		let (c_dealings, c_session) = wait_for_collection("charlie", &charlie_stats, 1, 0).await;
		let (d_dealings, d_session) = wait_for_collection("dave", &dave_stats, 1, 0).await;
		tracing::info!(
			"phase 1 done: charlie {c_dealings} dealings (session {c_session}), \
			 dave {d_dealings} dealings (session {d_session})"
		);

		// Phase 1.5: both collators submitted their own dealings to the
		// `key_gen` pallet, which merged them into the aggregated transcript.
		// (Must be checked within the first session: `AggregatedTranscript` is
		// not session-scoped yet — Phase 3.4.)
		let dealers = wait_for_aggregated_transcript(&charlie.client, 2).await;
		tracing::info!("on-chain aggregate has {dealers} dealers");

		// Phase 2: the session rotates (10 parachain blocks); both collators
		// deal again and collect a dealing from the new session.
		let next_session = c_session.max(d_session) + 1;
		wait_for_collection("charlie", &charlie_stats, c_dealings + 1, next_session).await;
		wait_for_collection("dave", &dave_stats, d_dealings + 1, next_session).await;
		tracing::info!("phase 2 done: dealings collected for session {next_session}");
	});
}
