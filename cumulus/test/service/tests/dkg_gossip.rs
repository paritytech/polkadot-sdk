//! In-process multi-node test for the DKG gadget.
//!
//! Spins up relay validators (Alice, Bob) and two parachain collators
//! (Charlie, Dave — deliberately different keyring identities than the relay
//! validators, same pattern as the transaction-throughput bench) and asserts
//! that the collators' DKG workers deal once per session and collect each
//! other's dealings over gossip, and that every session's round ends with an
//! on-chain threshold key aggregated from both collators' dealings.
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

use adkg_vrf::{bls::threshold::ThresholdVk, pvss::SecretSharing};
use ark_bls12_381::Bls12_381;
use ark_serialize::CanonicalDeserialize;
use codec::{Decode, Encode};
use cumulus_client_cli::get_raw_genesis_header;
use cumulus_primitives_core::ParaId;
use cumulus_test_service::{
	run_relay_chain_validator_node,
	runtime::ThresholdKey,
	Keyring::{Alice, Bob, Charlie, Dave},
	TestNodeBuilder,
};
use polkadot_primitives::HeadData;
use sc_client_api::StorageProvider;
use sc_dkg::CollectionStatsHandle;
use sp_core::storage::StorageKey;
use sp_crypto_hashing::{twox_128, twox_64};

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

/// Poll `KeyGen::ThresholdKeys` until the threshold key of the validator set of
/// `session` is on-chain; returns its number of dealers.
async fn wait_for_threshold_key(client: &cumulus_test_service::Client, session: u32) -> u32 {
	let session_key = session.encode();
	let key = StorageKey(
		[
			&twox_128(b"KeyGen")[..],
			&twox_128(b"ThresholdKeys")[..],
			&twox_64(&session_key)[..],
			&session_key[..],
		]
		.concat(),
	);
	tokio::time::timeout(Duration::from_secs(600), async {
		loop {
			let best_hash = client.chain_info().best_hash;
			let threshold_key = client
				.storage(best_hash, &key)
				.ok()
				.flatten()
				.and_then(|data| ThresholdKey::decode(&mut &data.0[..]).ok());
			if let Some(threshold_key) = threshold_key {
				// The stored key material is a valid secret sharing, from which
				// the VUF verification key derives.
				let secret_sharing = SecretSharing::<Bls12_381>::deserialize_compressed(
					&threshold_key.secret_sharing[..],
				)
				.expect("stored secret sharing deserializes");
				let _vk = ThresholdVk::from_share(&secret_sharing);
				return threshold_key.dealers;
			}
			tokio::time::sleep(Duration::from_secs(2)).await;
		}
	})
	.await
	.unwrap_or_else(|_| {
		dump_key_gen_events(client);
		panic!("timed out waiting for the threshold key of session {session}")
	})
}

/// Prints the `KeyGen` events and failed extrinsics of the best chain, oldest
/// first, to diagnose a missing threshold key.
fn dump_key_gen_events(client: &cumulus_test_service::Client) {
	use cumulus_test_service::runtime::{Hash, RuntimeEvent};
	use sp_blockchain::HeaderBackend;

	let events_key = StorageKey([twox_128(b"System"), twox_128(b"Events")].concat());
	let mut lines = Vec::new();
	let mut hash = client.chain_info().best_hash;
	while let Ok(Some(header)) = client.header(hash) {
		let records = client
			.storage(hash, &events_key)
			.ok()
			.flatten()
			.and_then(|data| {
				Vec::<frame_system::EventRecord<RuntimeEvent, Hash>>::decode(&mut &data.0[..]).ok()
			})
			.unwrap_or_default();
		for record in records {
			match record.event {
				RuntimeEvent::KeyGen(event) => lines.push(format!("#{}: {event:?}", header.number)),
				RuntimeEvent::System(frame_system::Event::ExtrinsicFailed {
					dispatch_error,
					..
				}) => lines.push(format!("#{}: ExtrinsicFailed {dispatch_error:?}", header.number)),
				_ => {},
			}
		}
		if header.number == 0 {
			break;
		}
		hash = header.parent_hash;
	}
	eprintln!("KeyGen events on the best chain:");
	for line in lines.iter().rev() {
		eprintln!("  {line}");
	}
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
		.block_on(
			alice.register_parachain(
				para_id,
				cumulus_test_service::runtime::WASM_BINARY
					.expect("You need to build the WASM binary to run this test!")
					.to_vec(),
				HeadData(
					get_raw_genesis_header(charlie.client.clone())
						.expect("Unable to get genesis HeadData."),
				),
			),
		)
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

		// Phase 2: both collators submitted their own dealings to the `key_gen`
		// pallet; at the session rotation (10 parachain blocks) the round ends
		// with the threshold key of the next validator set (6 invulnerables:
		// f = 1, so the 2 collator dealers meet the threshold).
		let first_session = c_session.max(d_session);
		let next_session = first_session + 1;
		let dealers = wait_for_threshold_key(&charlie.client, next_session).await;
		assert_eq!(dealers, 2, "both collators' dealings are in the key of session {next_session}");
		tracing::info!("phase 2 done: threshold key for session {next_session} on-chain");

		// Phase 3: both collators deal again and collect a dealing from the new
		// session.
		wait_for_collection("charlie", &charlie_stats, c_dealings + 1, next_session).await;
		wait_for_collection("dave", &dave_stats, d_dealings + 1, next_session).await;
		tracing::info!("phase 3 done: dealings collected for session {next_session}");

		// Phase 4: the new round is independent of the previous one (the
		// pallet would reject the re-deals as duplicates otherwise) and
		// produces the next session's key from the same two dealers.
		let dealers = wait_for_threshold_key(&charlie.client, next_session + 1).await;
		assert_eq!(dealers, 2, "the round of session {next_session} starts from scratch");
		tracing::info!("phase 4 done: threshold key for session {} on-chain", next_session + 1);
	});
}
