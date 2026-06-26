// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end test for the parachain collator reserved-peer mesh.
//!
//! Setup:
//! * 4 relay-chain validators on `westend-local`,
//! * 6 parachain collators running `test-parachain` with the `default-test` chain spec (default
//!   WASM — no `pallet_session` / `pallet_authority_discovery`), each launched with `--in-peers 1
//!   --out-peers 1 --collator-reserved-slots 32`. Plus 4 full nodes.
//!
//! Test flow:
//! 1. Spawn network (default, no-AD runtime). Wait for parachain block production.
//! 2. Perform `sudo set_code` upgrade to the `with-authority-discovery` variant WASM. The variant
//!    carries `spec_version = 4` (default = 2), triggering the `EnableAuthorityDiscovery` migration
//!    which seeds `pallet_session` from `pallet_aura::Authorities`.
//! 3. Wait for the runtime upgrade digest to appear in a finalised block.
//! 4. Wait for `AuthorityDiscovery.Keys` to become non-empty (migration fired). The on-chain AD set
//!    is populated, but no node yet has an `audi` keystore entry — the AD worker has nothing to
//!    publish under that key until step 5.
//! 5. Rotate AD keys for every collator via `author_insertKey` + `session.set_keys`. This puts a
//!    real `audi` key into each collator's keystore, enabling the AD worker to publish signed DHT
//!    records that the others can resolve.
//! 6. Wait for `AuthorityDiscovery.Keys` to reflect the rotation.
//! 7. Assert the full collator-to-collator reserved-peer mesh forms.

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assert_full_collator_mesh, assert_para_throughput,
	submit_extrinsic_and_wait_for_finalization_success, submit_sudo_runtime_upgrade,
	wait_for_authority_discovery_keys_change, wait_for_pallet_in_metadata, wait_for_pvf_prepare,
	wait_for_runtime_upgrade,
};
use polkadot_primitives::Id as ParaId;
use std::{collections::HashSet, str::FromStr, time::Duration};
use zombienet_sdk::{
	subxt::{
		backend::rpc::RpcClient, dynamic::Value, ext::subxt_rpcs::rpc_params, OnlineClient,
		PolkadotConfig,
	},
	subxt_signer::{sr25519::dev, SecretUri},
	LocalFileSystem, Network, NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 1000;

/// The variant WASM bytes — compiled with `with-authority-discovery` feature.
const VARIANT_WASM: Option<&[u8]> = cumulus_test_runtime::with_authority_discovery::WASM_BINARY;

/// Mesh convergence timeout after session change.
const FULL_MESH_TIMEOUT: Duration = Duration::from_secs(180);

/// Polling cadence for the full-mesh check.
const FULL_MESH_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Session-change wait timeout. `Period = 10` blocks × ~6 s ≈ 60 s per session;
/// 8 minutes is a comfortable ceiling for multiple rotations plus mesh convergence.
const SESSION_CHANGE_TIMEOUT: Duration = Duration::from_secs(480);

/// Tight non-reserved budget. With 4 full nodes in this network, the 1/1 slots allow
/// inbound and outbound connections, but every collator-to-collator link must go through
/// the reserved-peer bypass once the mesh forms.
const COLLATOR_NETWORK_ARGS: &[&str] = &[
	"-lparachain=debug,collator-discovery=debug,authority-discovery=debug",
	"--in-peers=1",
	"--out-peers=1",
	"--collator-reserved-slots=32",
	"--force-authoring",
	"--authoring=slot-based",
];

fn collator_names() -> &'static [&'static str] {
	&["alice", "bob", "charlie", "dave", "eve", "ferdie"]
}

fn full_node_names() -> &'static [&'static str] {
	&["full-node-0", "full-node-1", "full-node-2", "full-node-3"]
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_default_resources(|resources| {
					resources.with_request_cpu(2).with_request_memory("2G")
				})
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
				.with_validator(|node| node.with_name("validator-2"))
				.with_validator(|node| node.with_name("validator-3"))
		})
		.with_parachain(|p| {
			let names = collator_names();
			// `default-test` is a chain-spec id registered in the `test-parachain` binary's
			// `load_spec` function.  It loads the default `cumulus-test-runtime` WASM (no
			// pallet_session / pallet_authority_discovery) with para id 1000.
			let mut p = p
				.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("default-test")
				.with_collator(|n| {
					n.with_name(names[0])
						.validator(true)
						.with_args(COLLATOR_NETWORK_ARGS.iter().map(|a| (*a).into()).collect())
				});

			// Remaining 5 collators — reserved-mesh participants.
			for name in &names[1..] {
				let name = *name;
				p = p.with_collator(|n| {
					n.with_name(name)
						.validator(true)
						.with_args(COLLATOR_NETWORK_ARGS.iter().map(|a| (*a).into()).collect())
				});
			}

			// 4 full nodes — consume the 1/1 non-reserved slots but are not in the mesh.
			for name in full_node_names() {
				let name = *name;
				p = p.with_collator(|n| n.with_name(name).validator(false));
			}

			p
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})
}

/// Map a dev collator name to its sr25519 keypair.
fn dev_pair(name: &str) -> Result<zombienet_sdk::subxt_signer::sr25519::Keypair, anyhow::Error> {
	Ok(match name {
		"alice" => dev::alice(),
		"bob" => dev::bob(),
		"charlie" => dev::charlie(),
		"dave" => dev::dave(),
		"eve" => dev::eve(),
		"ferdie" => dev::ferdie(),
		other => return Err(anyhow!("unknown dev collator name: {other}")),
	})
}

/// Capitalise the first character of a collator name, e.g. `"alice"` → `"Alice"`.
fn capitalize(name: &str) -> String {
	let mut chars = name.chars();
	match chars.next() {
		Some(ch) => ch.to_uppercase().chain(chars).collect(),
		None => String::new(),
	}
}

/// For each collator: insert a fresh AD key (`//<Name>/rotated`) into the keystore and
/// re-submit `session.set_keys` with the same aura key + new AD key.
async fn rotate_authority_discovery_keys(
	network: &Network<LocalFileSystem>,
	names: &[&str],
) -> anyhow::Result<()> {
	// Key-type-id for authority-discovery: ASCII "audi" = 0x61756469.
	const AUDI_KEY_TYPE: &str = "audi";

	for &name in names {
		let cap = capitalize(name);

		// Derive the existing aura pubkey from `//<Name>` — keep it unchanged.
		let aura_uri = SecretUri::from_str(&format!("//{cap}"))
			.map_err(|e| anyhow!("bad aura URI for {name}: {e}"))?;
		let aura_pub: [u8; 32] = zombienet_sdk::subxt_signer::sr25519::Keypair::from_uri(&aura_uri)
			.map_err(|e| anyhow!("aura keypair for {name}: {e}"))?
			.public_key()
			.0;

		// Derive the new AD pubkey from `//<Name>/rotated`.
		let audi_uri_str = format!("//{cap}/rotated");
		let audi_uri = SecretUri::from_str(&audi_uri_str)
			.map_err(|e| anyhow!("bad audi URI for {name}: {e}"))?;
		let audi_pub: [u8; 32] = zombienet_sdk::subxt_signer::sr25519::Keypair::from_uri(&audi_uri)
			.map_err(|e| anyhow!("audi keypair for {name}: {e}"))?
			.public_key()
			.0;

		// Insert the new AD private key into the node's keystore via author_insertKey.
		let audi_pub_hex = sp_core::bytes::to_hex(&audi_pub, true);
		let rpc: RpcClient = network.get_node(name)?.rpc().await?;
		rpc.request::<()>(
			"author_insertKey",
			rpc_params![AUDI_KEY_TYPE, &audi_uri_str, &audi_pub_hex],
		)
		.await?;

		log::info!(
			"rotated AD-only for {name}: aura unchanged, new audi pub = 0x{}…",
			sp_core::bytes::to_hex(&audi_pub[..8], false),
		);

		// Construct the new SessionKeys value as a named composite matching the
		// on-chain metadata shape.
		let aura_value = Value::unnamed_composite([Value::from_bytes(aura_pub.as_slice())]);
		let audi_value = Value::unnamed_composite([Value::from_bytes(audi_pub.as_slice())]);
		let keys_value =
			Value::named_composite([("aura", aura_value), ("authority_discovery", audi_value)]);

		// Construct the ownership proof.
		const POP_TAG: &[u8; 4] = b"POP_";
		let owner: [u8; 32] = dev_pair(name)?.public_key().0;
		let mut statement = Vec::with_capacity(POP_TAG.len() + owner.len());
		statement.extend_from_slice(POP_TAG);
		statement.extend_from_slice(&owner);

		let aura_keypair = dev_pair(name)?;
		let audi_keypair = zombienet_sdk::subxt_signer::sr25519::Keypair::from_uri(
			&SecretUri::from_str(&audi_uri_str)?,
		)?;

		let aura_sig = aura_keypair.sign(&statement);
		let audi_sig = audi_keypair.sign(&statement);

		let mut proof_bytes = Vec::with_capacity(128);
		proof_bytes.extend_from_slice(&aura_sig.0);
		proof_bytes.extend_from_slice(&audi_sig.0);

		let call = zombienet_sdk::subxt::dynamic::tx(
			"Session",
			"set_keys",
			vec![keys_value, Value::from_bytes(proof_bytes)],
		);

		let signer = dev_pair(name)?;
		let para_client: OnlineClient<PolkadotConfig> =
			network.get_node(name)?.wait_client().await?;
		submit_extrinsic_and_wait_for_finalization_success(&para_client, &call, &signer).await?;

		log::info!("`{name}` set_keys submitted and finalised");
	}
	Ok(())
}

/// Assert each collator has caught up past block 10 — the cheap chain-advance sanity check.
async fn assert_collators_advance(
	network: &Network<LocalFileSystem>,
	names: &[&str],
) -> anyhow::Result<()> {
	for &name in names {
		let collator = network.get_node(name)?;
		log::info!("Asserting `{name}` reports a non-trivial best-block height");
		assert!(
			collator
				.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b > 10.0, 200u64)
				.await
				.is_ok(),
			"`{name}` did not reach best-block > 10",
		);
	}
	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn collator_discovery_full_mesh_with_tight_non_reserved_budget() -> Result<(), anyhow::Error>
{
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network (default runtime — no authority-discovery)");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	wait_for_pvf_prepare(&network, 1).await?;

	// Wait for parachain block production to confirm collators are talking to the relay chain.
	log::info!("Waiting for parachain block production (pre-upgrade)");
	assert_para_throughput(&relay_client, 10, [(ParaId::from(PARA_ID), 7..11)], []).await?;

	// Obtain a parachain client for storage queries.
	let para_node = network.get_node(collator_names()[0])?;
	let para_client: OnlineClient<PolkadotConfig> = para_node.wait_client().await?;

	// Step 1: upgrade to the with-authority-discovery variant WASM.
	// Alice is the sudo key (per the genesis preset).
	log::info!("Performing runtime upgrade to enable authority-discovery");
	let wasm = VARIANT_WASM.expect("with-authority-discovery WASM binary was not built");
	submit_sudo_runtime_upgrade(&para_client, wasm, &dev::alice()).await?;
	wait_for_runtime_upgrade(&para_client).await?;
	log::info!("Runtime upgrade applied");

	// After the runtime upgrade, recreate a subxt client whose metadata reflects the new
	// `AuthorityDiscovery` pallet so we can query its storage.
	let alice_ws = network.get_node("alice")?.ws_uri().to_string();
	let para_client = wait_for_pallet_in_metadata(
		&alice_ws,
		"AuthorityDiscovery",
		Duration::from_secs(120),
		Duration::from_secs(3),
	)
	.await?;

	// Step 2: wait for AD keys to appear (migration seeded them from aura authorities).
	// Pre-upgrade the initial AD key set is empty (pallet not present).
	log::info!("Waiting for authority-discovery keys to populate post-upgrade");
	let initial_authorities = wait_for_authority_discovery_keys_change(
		&para_client,
		&HashSet::new(),
		SESSION_CHANGE_TIMEOUT,
		Duration::from_secs(3),
	)
	.await?;
	log::info!(
		"Captured {} migration-seeded AD authorities; rotating to real AD keys",
		initial_authorities.len(),
	);

	// Step 3: rotate. The migration seeds `AuthorityDiscovery::Keys` with the aura pubkeys,
	// but no node has the matching `audi` private key yet, so the AD worker can't publish
	// DHT records until we rotate to real keys.
	rotate_authority_discovery_keys(&network, collator_names()).await?;

	// Step 4: wait for AuthorityDiscovery.Keys to reflect the rotation.
	log::info!("Waiting for AuthorityDiscovery keys to reflect the rotation");
	wait_for_authority_discovery_keys_change(
		&para_client,
		&initial_authorities,
		SESSION_CHANGE_TIMEOUT,
		Duration::from_secs(3),
	)
	.await?;

	// Step 5: assert the full collator-to-collator mesh forms with real AD keys in place.
	log::info!("Asserting full collator-to-collator mesh");
	assert_full_collator_mesh(
		&network,
		collator_names(),
		FULL_MESH_TIMEOUT,
		FULL_MESH_POLL_INTERVAL,
	)
	.await?;
	assert_collators_advance(&network, collator_names()).await?;

	// Step 6: parachain throughput remains healthy with the discovery mesh active.
	wait_for_pvf_prepare(&network, 2).await?;
	log::info!("Checking parachain throughput post-discovery");
	assert_para_throughput(&relay_client, 10, [(ParaId::from(PARA_ID), 7..11)], []).await?;

	log::info!("Test finished successfully.");
	Ok(())
}
