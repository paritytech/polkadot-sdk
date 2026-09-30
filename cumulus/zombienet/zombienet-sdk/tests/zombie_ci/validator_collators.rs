// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Relay chain validators that register session keys on Asset Hub and People collate there.
//!
//! Setup:
//! * `westend-local` with validators `alice` and `bob`, whose session validators are AliceStash and
//!   BobStash. `pallet-staking-async-ah-client` starts in `Active` mode. The test opens HRMP
//!   channels between Asset Hub and People with `sudo`.
//! * `asset-hub-westend-local` electing two validators from its genesis stakers AliceStash and
//!   BobStash, with the signed election phases skipped and a short unsigned phase.
//! * `people-westend-local`.
//! * On each parachain one invulnerable collator, plus one collator per validator that is not a
//!   collator at genesis.
//!
//! Test flow:
//! 1. AliceStash and BobStash register the keys of their collator nodes on both parachains.
//! 2. Asset Hub elects AliceStash and BobStash, the relay chain applies the set, and era 1 starts
//!    on Asset Hub. Asset Hub stores the set and announces it to People.
//! 3. Both parachains rotate AliceStash and BobStash in as collators, and their collator nodes
//!    author blocks.

use crate::utils::initialize_network;

use anyhow::anyhow;
use codec::{Decode, Encode};
use cumulus_zombienet_sdk_helpers::{
	open_hrmp_channel, submit_extrinsic_and_wait_for_finalization_success,
};
use serde::Deserialize;
use serde_json::json;
use sp_core::Bytes;
use std::{collections::BTreeSet, str::FromStr, time::Duration};
use tokio::time::Instant;
use zombienet_sdk::{
	subxt::{
		self,
		backend::rpc::RpcClient,
		dynamic::Value,
		ext::{
			scale_value::{At, Composite, ValueDef},
			subxt_rpcs::rpc_params,
		},
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::{
		sr25519::{dev, Keypair},
		SecretUri,
	},
	LocalFileSystem, Network, NetworkConfig, NetworkConfigBuilder,
};

const ASSET_HUB_ID: u32 = 1000;
const PEOPLE_ID: u32 = 1004;

/// Length of the unsigned election phase on Asset Hub, in blocks.
const UNSIGNED_PHASE: u32 = 20;

/// For each sudo call on the relay chain and each RPC request to a collator node.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// From `check_preconditions`, which runs after the parachain-block wait, to the relay chain
/// session change that opens the HRMP channels. Relay chain sessions last 2 minutes.
const HRMP_OPEN_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// Blocks each parachain must finalize before the test registers keys.
const PARA_BLOCKS: u32 = 3;
const PARA_BLOCKS_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// For all four key registrations.
const KEY_REGISTRATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// From spawning the event watchers, just before key registration, to era 1 starting on Asset
/// Hub. Asset Hub exports the elected set once the report of relay chain session 4 arrives, and
/// the relay chain applies it two session changes later.
const ERA_START_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// From era 1 starting on Asset Hub to People receiving the set.
const PEOPLE_RECEIVE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// From receiving the set to both forced rotations having happened.
const SESSION_ROTATION_TIMEOUT: Duration = Duration::from_secs(2 * 60);

/// From the rotations to both validator collators having authored a block on both parachains.
const AUTHORSHIP_TIMEOUT: Duration = Duration::from_secs(3 * 60);

const POLL_INTERVAL: Duration = Duration::from_secs(6);

/// A relay chain validator: the relay node, its stash and its collator node on each parachain.
struct Validator {
	stash_uri: &'static str,
	asset_hub_collator: &'static str,
	people_collator: &'static str,
}

const VALIDATORS: [Validator; 2] = [
	Validator {
		stash_uri: "//Alice//stash",
		asset_hub_collator: "asset-hub-alice",
		people_collator: "people-alice",
	},
	Validator {
		stash_uri: "//Bob//stash",
		asset_hub_collator: "asset-hub-bob",
		people_collator: "people-bob",
	},
];

const ASSET_HUB_INVULNERABLE: &str = "asset-hub-collator";
const PEOPLE_INVULNERABLE: &str = "people-collator";

/// Response of `author_rotateKeysWithOwner`.
#[derive(Deserialize)]
struct GeneratedSessionKeys {
	keys: Bytes,
	proof: Option<Bytes>,
}

#[tokio::test(flavor = "multi_thread")]
async fn validator_collators_on_asset_hub_and_people() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;
	network.wait_until_is_up(120).await?;

	let relay_client: OnlineClient<PolkadotConfig> =
		network.get_node("alice")?.wait_client().await?;
	let asset_hub_client: OnlineClient<PolkadotConfig> =
		network.get_node(ASSET_HUB_INVULNERABLE)?.wait_client().await?;
	let people_client: OnlineClient<PolkadotConfig> =
		network.get_node(PEOPLE_INVULNERABLE)?.wait_client().await?;

	open_hrmp_channels(&relay_client).await?;

	log::info!("Waiting for both parachains to finalize {PARA_BLOCKS} blocks");
	tokio::try_join!(
		wait_for_finalized_block(&asset_hub_client, "Asset Hub", PARA_BLOCKS),
		wait_for_finalized_block(&people_client, "People", PARA_BLOCKS),
	)?;

	// GIVEN a network where Asset Hub has not started era 1 yet, elects two validators, reports
	// to a relay chain in `Active` mode and can send to People
	check_preconditions(&relay_client, &asset_hub_client).await?;

	let asset_hub_received = tokio::spawn(wait_for_event(
		asset_hub_client.clone(),
		"Asset Hub",
		"ValidatorCollators",
		"ValidatorSetReceived",
		is_set_of_era_1_with_two_validators,
		ERA_START_TIMEOUT,
	));
	let asset_hub_announced = tokio::spawn(wait_for_event(
		asset_hub_client.clone(),
		"Asset Hub",
		"ValidatorSetAnnouncer",
		"AnnouncementSent",
		is_announcement_of_era_1,
		ERA_START_TIMEOUT,
	));
	let people_received = tokio::spawn(wait_for_event(
		people_client.clone(),
		"People",
		"ValidatorCollators",
		"ValidatorSetReceived",
		is_set_of_era_1_with_two_validators,
		ERA_START_TIMEOUT + PEOPLE_RECEIVE_TIMEOUT,
	));

	// AND AliceStash and BobStash registered the keys of their collator nodes on both parachains
	let registration = async {
		for validator in &VALIDATORS {
			let stash = keypair(validator.stash_uri)?;
			register_keys(&network, &asset_hub_client, validator.asset_hub_collator, &stash)
				.await?;
			register_keys(&network, &people_client, validator.people_collator, &stash).await?;
		}
		Ok::<_, anyhow::Error>(())
	};
	tokio::time::timeout(KEY_REGISTRATION_TIMEOUT, registration)
		.await
		.map_err(|_| {
			anyhow!("key registration did not finish within {KEY_REGISTRATION_TIMEOUT:?}")
		})??;
	let active_era = active_era(&asset_hub_client).await?;
	if active_era != 0 {
		return Err(anyhow!("keys must be registered before era 1, active era is {active_era}"));
	}

	// WHEN Asset Hub starts era 1 with AliceStash and BobStash
	log::info!("Waiting for era 1 to start on Asset Hub");
	let asset_hub_received_at = asset_hub_received.await??;
	asset_hub_announced.await??;
	log::info!("Waiting for People to receive the set of era 1");
	let people_received_at =
		tokio::time::timeout(PEOPLE_RECEIVE_TIMEOUT, people_received).await.map_err(
			|_| anyhow!("People did not receive the set within {PEOPLE_RECEIVE_TIMEOUT:?}"),
		)???;

	// THEN both parachains collate with their invulnerable, AliceStash and BobStash
	let stashes = VALIDATORS
		.iter()
		.map(|validator| Ok(account(&keypair(validator.stash_uri)?)))
		.collect::<Result<Vec<_>, anyhow::Error>>()?;
	for (client, chain, invulnerable) in [
		(&asset_hub_client, "Asset Hub", ASSET_HUB_INVULNERABLE),
		(&people_client, "People", PEOPLE_INVULNERABLE),
	] {
		let expected = stashes
			.iter()
			.copied()
			.chain([account(&keypair(&node_seed(invulnerable))?)])
			.collect::<BTreeSet<_>>();
		wait_for_session_validators(client, chain, &expected).await?;
	}

	// AND the collator nodes of AliceStash and BobStash author blocks on both parachains
	let deadline = Instant::now() + AUTHORSHIP_TIMEOUT;
	let authorship = [
		(&asset_hub_client, "Asset Hub", asset_hub_received_at),
		(&people_client, "People", people_received_at),
	]
	.into_iter()
	.flat_map(|(client, chain, received_at)| {
		stashes
			.iter()
			.map(move |stash| wait_for_authored_block(client, chain, stash, received_at, deadline))
	});
	futures::future::try_join_all(authorship).await?;

	// AND the relay chain kept its validators, since the elected set is the running one
	let relay_validators = session_validators(&relay_client).await?;
	if relay_validators != stashes.iter().copied().collect::<BTreeSet<_>>() {
		return Err(anyhow!("relay chain session validators changed: {relay_validators:?}"));
	}

	log::info!("Test finished successfully");
	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=info,runtime::staking-async::ah-client=debug,xcm=info").into(),
				])
				.with_genesis_overrides(json!({
					"stakingAhClient": { "operatingMode": "Active" }
				}))
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(ASSET_HUB_ID)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("asset-hub-westend-local")
				.with_default_args(collator_args(
					"runtime::multiblock-election=debug,runtime::staking-async=debug,\
					runtime::staking-async::rc-client=debug,runtime::validator-collators=debug,\
					runtime::validator-set-announcer=debug",
				))
				.with_genesis_overrides(json!({
					"staking": { "validatorCount": VALIDATORS.len(), "devStakers": null }
				}))
				.with_raw_spec_override(election_phases_override())
				.with_collator(|n| n.with_name(ASSET_HUB_INVULNERABLE))
				.with_collator(|n| {
					n.with_name(VALIDATORS[0].asset_hub_collator).invulnerable(false)
				})
				.with_collator(|n| {
					n.with_name(VALIDATORS[1].asset_hub_collator).invulnerable(false)
				})
		})
		.with_parachain(|p| {
			p.with_id(PEOPLE_ID)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("people-westend-local")
				.with_default_args(collator_args("runtime::validator-collators=debug"))
				.with_collator(|n| n.with_name(PEOPLE_INVULNERABLE))
				.with_collator(|n| n.with_name(VALIDATORS[0].people_collator).invulnerable(false))
				.with_collator(|n| n.with_name(VALIDATORS[1].people_collator).invulnerable(false))
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

fn collator_args(log_targets: &str) -> Vec<zombienet_sdk::Arg> {
	vec![
		format!("-lparachain=info,aura=debug,xcm=info,{log_targets}").as_str().into(),
		("--force-authoring").into(),
		("--authoring", "slot-based").into(),
	]
}

/// Skips the signed election phases on Asset Hub and shortens the unsigned one, by writing the
/// `pub storage` parameters `SignedPhase`, `UnsignedPhase` and `SignedValidationPhase` into the
/// raw genesis.
fn election_phases_override() -> serde_json::Value {
	let entry = |name: &str, value: u32| {
		let key = sp_crypto_hashing::twox_128(format!(":{name}:").as_bytes());
		(format!("0x{}", hex::encode(key)), json!(format!("0x{}", hex::encode(value.encode()))))
	};
	let top = serde_json::Map::from_iter([
		entry("SignedPhase", 0),
		entry("UnsignedPhase", UNSIGNED_PHASE),
		entry("SignedValidationPhase", 0),
	]);
	json!({ "genesis": { "raw": { "top": top } } })
}

async fn check_preconditions(
	relay_client: &OnlineClient<PolkadotConfig>,
	asset_hub_client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	let validator_count: u32 = fetch(asset_hub_client, "Staking", "ValidatorCount", vec![])
		.await?
		.ok_or_else(|| anyhow!("Asset Hub `Staking::ValidatorCount` is not set"))?;
	let active_era = active_era(asset_hub_client).await?;
	// `OperatingMode` is encoded as its variant index: `Passive`, `Buffered`, `Active`.
	let mode: u8 = fetch(relay_client, "StakingAhClient", "Mode", vec![]).await?.unwrap_or(0);
	log::info!(
		"Asset Hub validator count {validator_count}, active era {active_era}, relay ah-client \
		mode {mode}"
	);

	if validator_count != VALIDATORS.len() as u32 {
		return Err(anyhow!("Asset Hub validator count is {validator_count}"));
	}
	if active_era != 0 {
		return Err(anyhow!("Asset Hub active era is {active_era}, expected 0"));
	}
	if mode != 2 {
		return Err(anyhow!("relay ah-client mode is {mode}, expected `Active`"));
	}
	wait_for_hrmp_channel(relay_client, ASSET_HUB_ID, PEOPLE_ID).await
}

/// Opens the HRMP channels from Asset Hub to People and back. They open at the next relay chain
/// session change. The channel back lets People answer Asset Hub's version subscription.
async fn open_hrmp_channels(
	relay_client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	for (sender, recipient) in [(ASSET_HUB_ID, PEOPLE_ID), (PEOPLE_ID, ASSET_HUB_ID)] {
		// `max_capacity` is the preset's `hrmp_channel_max_capacity`.
		open_hrmp_channel(
			relay_client,
			sender,
			recipient,
			8,
			1024,
			&dev::alice(),
			CALL_TIMEOUT.as_secs(),
		)
		.await?;
		log::info!("Requested HRMP channel {sender} to {recipient}");
	}
	Ok(())
}

async fn wait_for_hrmp_channel(
	relay_client: &OnlineClient<PolkadotConfig>,
	sender: u32,
	recipient: u32,
) -> Result<(), anyhow::Error> {
	let wait = async {
		loop {
			let channel = Value::named_composite([
				("sender", Value::u128(sender.into())),
				("recipient", Value::u128(recipient.into())),
			]);
			if fetch_raw(relay_client, "Hrmp", "HrmpChannels", vec![channel]).await?.is_some() {
				log::info!("HRMP channel {sender} to {recipient} is open");
				return Ok::<_, anyhow::Error>(());
			}
			tokio::time::sleep(POLL_INTERVAL).await;
		}
	};
	tokio::time::timeout(HRMP_OPEN_TIMEOUT, wait).await.map_err(|_| {
		anyhow!("HRMP channel {sender} to {recipient} not open within {HRMP_OPEN_TIMEOUT:?}")
	})?
}

/// Generates keys on `collator` owned by `stash` and registers them from `stash`.
async fn register_keys(
	network: &Network<LocalFileSystem>,
	client: &OnlineClient<PolkadotConfig>,
	collator: &str,
	stash: &Keypair,
) -> Result<(), anyhow::Error> {
	let owner = account(stash);
	let rpc: RpcClient = network.get_node(collator)?.rpc().await?;
	let generated: GeneratedSessionKeys = tokio::time::timeout(
		CALL_TIMEOUT,
		rpc.request("author_rotateKeysWithOwner", rpc_params![Bytes(owner.to_vec())]),
	)
	.await
	.map_err(|_| anyhow!("`{collator}` did not rotate keys within {CALL_TIMEOUT:?}"))??;
	let proof = generated
		.proof
		.ok_or_else(|| anyhow!("`{collator}` returned keys without an ownership proof"))?;
	// The session keys of both parachains are one sr25519 Aura key.
	let aura: [u8; 32] = generated.keys.0.as_slice().try_into().map_err(|_| {
		anyhow!("`{collator}` returned {} bytes of keys, expected 32", generated.keys.0.len())
	})?;
	log::info!(
		"`{collator}` generated Aura key 0x{} for 0x{}",
		hex::encode(aura),
		hex::encode(owner)
	);

	let keys = Value::named_composite([(
		"aura",
		Value::unnamed_composite([Value::from_bytes(aura.as_slice())]),
	)]);
	let call = subxt::dynamic::tx(
		"Session",
		"set_keys",
		vec![keys, Value::from_bytes(proof.0.as_slice())],
	);
	submit_extrinsic_and_wait_for_finalization_success(client, &call, stash).await?;

	let registered = fetch_raw(client, "Session", "NextKeys", vec![Value::from_bytes(owner)])
		.await?
		.is_some();
	if !registered {
		return Err(anyhow!("`Session::NextKeys` of 0x{} is not set", hex::encode(owner)));
	}
	log::info!("0x{} registered the keys of `{collator}`", hex::encode(owner));
	Ok(())
}

/// Waits for an event of `pallet` and `variant` whose fields satisfy `matches`, and returns the
/// number of the finalized block holding it.
async fn wait_for_event(
	client: OnlineClient<PolkadotConfig>,
	chain: &'static str,
	pallet: &'static str,
	variant: &'static str,
	matches: fn(&Composite<u32>) -> bool,
	timeout: Duration,
) -> Result<u32, anyhow::Error> {
	let wait = async {
		let mut blocks = client.blocks().subscribe_finalized().await?;
		while let Some(block) = blocks.next().await {
			let block = block?;
			if block.number() % 50 == 0 {
				log::info!("{chain} at #{}, waiting for `{pallet}::{variant}`", block.number());
			}
			for event in block.events().await?.iter() {
				let event = event?;
				if event.pallet_name() != pallet || event.variant_name() != variant {
					continue;
				}
				let fields = event.field_values()?;
				if matches(&fields) {
					log::info!("{chain} #{}: `{pallet}::{variant}` {fields:?}", block.number());
					return Ok(block.number());
				}
			}
		}
		Err(anyhow!("{chain} block subscription ended"))
	};
	tokio::time::timeout(timeout, wait)
		.await
		.map_err(|_| anyhow!("no `{pallet}::{variant}` on {chain} within {timeout:?}"))?
}

/// `ValidatorSetReceived { era: 1, count: 2 }`.
fn is_set_of_era_1_with_two_validators(fields: &Composite<u32>) -> bool {
	fields.at("era").and_then(|era| era.as_u128()) == Some(1) &&
		fields.at("count").and_then(|count| count.as_u128()) == Some(VALIDATORS.len() as u128)
}

/// `AnnouncementSent { destination: People, era: 1 }`.
fn is_announcement_of_era_1(fields: &Composite<u32>) -> bool {
	let to_people = fields.at("destination").is_some_and(
		|destination| matches!(&destination.value, ValueDef::Variant(v) if v.name == "People"),
	);
	to_people && fields.at("era").and_then(|era| era.as_u128()) == Some(1)
}

async fn wait_for_finalized_block(
	client: &OnlineClient<PolkadotConfig>,
	chain: &str,
	number: u32,
) -> Result<(), anyhow::Error> {
	let wait = async {
		let mut blocks = client.blocks().subscribe_finalized().await?;
		while let Some(block) = blocks.next().await {
			if block?.number() >= number {
				log::info!("{chain} finalized block #{number}");
				return Ok(());
			}
		}
		Err(anyhow!("{chain} block subscription ended"))
	};
	tokio::time::timeout(PARA_BLOCKS_TIMEOUT, wait)
		.await
		.map_err(|_| anyhow!("{chain} did not finalize #{number} within {PARA_BLOCKS_TIMEOUT:?}"))?
}

async fn wait_for_session_validators(
	client: &OnlineClient<PolkadotConfig>,
	chain: &str,
	expected: &BTreeSet<[u8; 32]>,
) -> Result<(), anyhow::Error> {
	let wait = async {
		loop {
			let validators = session_validators(client).await?;
			if &validators == expected {
				log::info!("{chain} session validators: {}", hex_accounts(&validators));
				return Ok::<_, anyhow::Error>(());
			}
			log::info!("{chain} session validators still {}", hex_accounts(&validators));
			tokio::time::sleep(POLL_INTERVAL).await;
		}
	};
	tokio::time::timeout(SESSION_ROTATION_TIMEOUT, wait).await.map_err(|_| {
		anyhow!(
			"{chain} session validators did not become {} within {SESSION_ROTATION_TIMEOUT:?}",
			hex_accounts(expected)
		)
	})?
}

/// Waits until `author` authored a block after block `after`, until `deadline`.
async fn wait_for_authored_block(
	client: &OnlineClient<PolkadotConfig>,
	chain: &str,
	author: &[u8; 32],
	after: u32,
	deadline: Instant,
) -> Result<(), anyhow::Error> {
	let wait = async {
		loop {
			let last: Option<u32> = fetch(
				client,
				"CollatorSelection",
				"LastAuthoredBlock",
				vec![Value::from_bytes(author)],
			)
			.await?;
			if let Some(block) = last.filter(|block| *block > after) {
				log::info!("{chain}: 0x{} last authored block #{block}", hex::encode(author));
				return Ok::<_, anyhow::Error>(());
			}
			tokio::time::sleep(POLL_INTERVAL).await;
		}
	};
	tokio::time::timeout_at(deadline, wait).await.map_err(|_| {
		anyhow!(
			"0x{} authored no block on {chain} after #{after} within {AUTHORSHIP_TIMEOUT:?}",
			hex::encode(author)
		)
	})?
}

async fn active_era(client: &OnlineClient<PolkadotConfig>) -> Result<u32, anyhow::Error> {
	// `ActiveEraInfo { index, start }`.
	let (index, _start): (u32, Option<u64>) = fetch(client, "Staking", "ActiveEra", vec![])
		.await?
		.ok_or_else(|| anyhow!("`Staking::ActiveEra` is not set"))?;
	Ok(index)
}

async fn session_validators(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<BTreeSet<[u8; 32]>, anyhow::Error> {
	let validators: Vec<[u8; 32]> =
		fetch(client, "Session", "Validators", vec![]).await?.unwrap_or_default();
	Ok(validators.into_iter().collect())
}

async fn fetch<T: Decode>(
	client: &OnlineClient<PolkadotConfig>,
	pallet: &str,
	item: &str,
	keys: Vec<Value>,
) -> Result<Option<T>, anyhow::Error> {
	fetch_raw(client, pallet, item, keys)
		.await?
		.map(|encoded| {
			T::decode(&mut &encoded[..]).map_err(|e| anyhow!("decode `{pallet}::{item}`: {e}"))
		})
		.transpose()
}

async fn fetch_raw(
	client: &OnlineClient<PolkadotConfig>,
	pallet: &str,
	item: &str,
	keys: Vec<Value>,
) -> Result<Option<Vec<u8>>, anyhow::Error> {
	let query = subxt::dynamic::storage(pallet, item, keys);
	let value = client.storage().at_latest().await?.fetch(&query).await?;
	Ok(value.map(|value| value.into_encoded()))
}

fn keypair(uri: &str) -> Result<Keypair, anyhow::Error> {
	Ok(Keypair::from_uri(&SecretUri::from_str(uri)?)?)
}

fn account(keypair: &Keypair) -> [u8; 32] {
	keypair.public_key().0
}

/// The seed zombienet derives a node's keys from: its name with the first letter capitalised.
fn node_seed(name: &str) -> String {
	let mut chars = name.chars();
	let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
	format!("//{first}{}", chars.as_str())
}

fn hex_accounts(accounts: &BTreeSet<[u8; 32]>) -> String {
	accounts
		.iter()
		.map(|a| format!("0x{}", hex::encode(a)))
		.collect::<Vec<_>>()
		.join(", ")
}
