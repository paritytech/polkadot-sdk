//! In-process multi-node test for the DKG between relay-chain validators.
//!
//! Spins up relay validators (Alice, Bob) running the DKG gadget and two
//! parachain collators (Charlie, Dave) of the aggregation parachain. In every
//! relay session the validators deal with their BEEFY keys, gossip their
//! dealings to each other and submit them over JSON-RPC (Charlie's endpoint)
//! as unsigned extrinsics; the parachain's `key_gen` pallet follows the relay
//! session and validator set through the relay state proof and aggregates the
//! dealings into the threshold key of the next relay validator set.
//!
//! Only compiled with the `dkg` feature. Run with:
//! `cargo test -p cumulus-test-service --features dkg --test dkg_gossip`
#![cfg(feature = "dkg")]

use std::{ops::RangeInclusive, sync::atomic::Ordering, time::Duration};

use adkg_vrf::{bls::threshold::ThresholdVk, pvss::SecretSharing};
use ark_bls12_381::Bls12_381;
use ark_serialize::CanonicalDeserialize;
use codec::{Decode, Encode};
use cumulus_client_cli::get_raw_genesis_header;
use cumulus_primitives_core::ParaId;
use cumulus_test_service::{
	run_relay_chain_validator_node_with_dkg,
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

/// Relay sessions (BABE epochs) of 20 six-second slots instead of the test
/// runtime's 5, so that each round has room for the parachain to see the new
/// session and its validator set, and for the dealings to be included.
const RELAY_EPOCH_SLOTS: u64 = 20;

/// Relay-chain genesis tweak, applied by every relay node (validators and the
/// collators' embedded relay nodes) so they agree on the genesis.
fn longer_relay_sessions() {
	polkadot_test_runtime::EpochDuration::set(&RELAY_EPOCH_SLOTS);
}

/// A currently free local TCP port.
fn free_port() -> u16 {
	std::net::TcpListener::bind("127.0.0.1:0")
		.and_then(|listener| listener.local_addr())
		.expect("can bind a local port")
		.port()
}

/// Poll `stats` until the validator collected at least one dealing over gossip.
async fn wait_for_collection(node: &str, stats: &CollectionStatsHandle) -> u32 {
	tokio::time::timeout(Duration::from_secs(300), async {
		loop {
			if stats.dealings.load(Ordering::Relaxed) > 0 {
				return stats.latest_session.load(Ordering::Relaxed);
			}
			tokio::time::sleep(Duration::from_secs(1)).await;
		}
	})
	.await
	.unwrap_or_else(|_| panic!("{node}: timed out waiting for a gossiped dealing"))
}

/// Poll `KeyGen::ThresholdKeys` until the threshold key of the relay validator
/// set of a session in `sessions` is on-chain; returns the session and the
/// key's number of dealers.
async fn wait_for_threshold_key(
	client: &cumulus_test_service::Client,
	sessions: RangeInclusive<u32>,
	timeout: Duration,
) -> (u32, u32) {
	let storage_key = |session: u32| {
		let session = session.encode();
		StorageKey(
			[
				&twox_128(b"KeyGen")[..],
				&twox_128(b"ThresholdKeys")[..],
				&twox_64(&session)[..],
				&session[..],
			]
			.concat(),
		)
	};
	tokio::time::timeout(timeout, async {
		loop {
			let best_hash = client.chain_info().best_hash;
			for session in sessions.clone() {
				let threshold_key = client
					.storage(best_hash, &storage_key(session))
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
					return (session, threshold_key.dealers);
				}
			}
			tokio::time::sleep(Duration::from_secs(2)).await;
		}
	})
	.await
	.unwrap_or_else(|_| {
		dump_key_gen_events(client);
		panic!("timed out waiting for a threshold key of a session in {sessions:?}")
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
fn relay_validators_deal_and_the_parachain_aggregates() {
	sp_tracing::try_init_simple();
	let mut builder = sc_cli::LoggerBuilder::new("");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(PARA_ID);
	// `run_relay_chain_validator_node_with_dkg` does `handle.block_on`
	// internally, so it must be called from outside the runtime (same pattern
	// as the transaction-throughput bench).
	let runtime = tokio::runtime::Runtime::new().expect("Creates tokio runtime");
	let tokio_handle = runtime.handle().clone();

	// The relay validators submit their dealings to Charlie's RPC endpoint
	// (retrying until Charlie is up and the parachain has started the round).
	let rpc_port = free_port();
	let dealings_rpc_url = format!("ws://127.0.0.1:{rpc_port}");

	let (alice, alice_stats) = run_relay_chain_validator_node_with_dkg(
		tokio_handle.clone(),
		Alice,
		longer_relay_sessions,
		vec![],
		dealings_rpc_url.clone(),
	);
	let (bob, bob_stats) = run_relay_chain_validator_node_with_dkg(
		tokio_handle.clone(),
		Bob,
		longer_relay_sessions,
		vec![alice.addr.clone()],
		dealings_rpc_url,
	);

	// First collator, serving the JSON-RPC endpoint the validators submit to.
	let charlie = runtime.block_on(
		TestNodeBuilder::new(para_id, tokio_handle.clone(), Charlie)
			.enable_collator()
			.enable_rpc(rpc_port)
			.update_storage_relay_chain(longer_relay_sessions)
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

	// Second collator (the test runtime's Aura authorities include both).
	let dave = runtime.block_on(
		TestNodeBuilder::new(para_id, tokio_handle.clone(), Dave)
			.enable_collator()
			.update_storage_relay_chain(longer_relay_sessions)
			.connect_to_parachain_node(&charlie)
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build(),
	);

	runtime.block_on(async {
		// Phase 1: the validators deal to each other: each collects the other's
		// dealing over gossip.
		let alice_session = wait_for_collection("alice", &alice_stats).await;
		let bob_session = wait_for_collection("bob", &bob_stats).await;
		tracing::info!(
			"phase 1 done: alice collected a dealing of session {alice_session}, \
			 bob one of session {bob_session}"
		);

		// Phase 2: once the parachain runs, a round ends with the threshold key
		// of the next relay validator set, aggregated from both validators'
		// dealings (2 validators: f = 0, so one dealer would suffice).
		dave.wait_for_blocks(1).await;
		let (first, dealers) =
			wait_for_threshold_key(&charlie.client, 1..=30, Duration::from_secs(900)).await;
		assert_eq!(dealers, 2, "both validators' dealings are in the key of session {first}");
		tracing::info!("phase 2 done: threshold key for relay session {first} on-chain");

		// Phase 3: the next round starts from scratch (the pallet would reject
		// the re-deals as duplicates otherwise) and yields the next session's key.
		let (next, dealers) = wait_for_threshold_key(
			&charlie.client,
			first + 1..=first + 1,
			Duration::from_secs(600),
		)
		.await;
		assert_eq!(dealers, 2, "the round of relay session {} starts from scratch", next - 1);
		tracing::info!("phase 3 done: threshold key for relay session {next} on-chain");
	});
}
