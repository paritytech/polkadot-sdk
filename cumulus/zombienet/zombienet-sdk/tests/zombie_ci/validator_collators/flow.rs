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
//! * Both parachains report authored blocks every 10 blocks and expect a 12-minute era, in place of
//!   the runtime defaults, so the report before the expected era end comes within the test.
//! * On each parachain one invulnerable collator, plus one collator per validator that is not a
//!   collator at genesis.
//!
//! Test flow:
//! 1. AliceStash and BobStash register the keys of their collator nodes on both parachains.
//! 2. Asset Hub elects AliceStash and BobStash, the relay chain applies the set, and era 1 starts
//!    on Asset Hub. Asset Hub stores the set and announces it to People.
//! 3. Both parachains rotate AliceStash and BobStash in as collators, and their collator nodes
//!    author blocks.
//! 4. Both parachains report the blocks of AliceStash and BobStash, and Asset Hub credits them as
//!    era points of the active era.
//! 5. Both parachains report once before the expected end of era 1.

use super::common::{
	account, build_network_config, fetch, fetch_raw, fetch_raw_at, hex_accounts, init_logging,
	is_announcement_of_era_1, keypair, node_seed, open_hrmp_channels, session_validators,
	wait_for_event, wait_for_finalized_block, wait_for_hrmp_channel, NetworkOptions, ASSET_HUB_ID,
	ASSET_HUB_INVULNERABLE, CALL_TIMEOUT, CLIENT_TIMEOUT_SECS, PARA_BLOCKS, PEOPLE_ID,
	PEOPLE_INVULNERABLE, POLL_INTERVAL, VALIDATORS,
};
use crate::utils::initialize_network;

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::submit_extrinsic_and_wait_for_finalization_success;
use serde::Deserialize;
use sp_core::Bytes;
use std::{collections::BTreeSet, time::Duration};
use tokio::time::Instant;
use zombienet_sdk::{
	subxt::{
		self,
		backend::rpc::RpcClient,
		dynamic::Value,
		ext::{
			scale_value::{At, Composite},
			subxt_rpcs::rpc_params,
		},
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::Keypair,
	LocalFileSystem, Network,
};

/// Largest HRMP message between the two chains. The announcement of two validators fits.
const HRMP_MAX_MESSAGE_SIZE: u32 = 1024;

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

/// `ValidatorCollators::ReportPeriod` of both parachains, in blocks.
const REPORT_PERIOD: u32 = 10;

/// From the authorship check to Asset Hub crediting the blocks of each parachain: one report
/// period plus a margin for the XCM delivery.
const CREDIT_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// `ValidatorCollators::ExpectedEraDuration` of both parachains, in milliseconds: the CI era of
/// six relay chain sessions of 2 minutes.
const EXPECTED_ERA_DURATION_MILLIS: u64 = 720_000;

/// From a parachain receiving the set of era 1 to its report before the expected end of the era:
/// one CI era plus a margin for finality.
const EARLY_REPORT_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Response of `author_rotateKeysWithOwner`.
#[derive(Deserialize)]
struct GeneratedSessionKeys {
	keys: Bytes,
	proof: Option<Bytes>,
}

/// The whole test, below the 60-minute CI step so that the step ends with this error and the log
/// collection steps run.
const TEST_TIMEOUT: Duration = Duration::from_secs(55 * 60);

#[tokio::test(flavor = "multi_thread")]
async fn validator_collators_on_asset_hub_and_people() -> Result<(), anyhow::Error> {
	init_logging();

	tokio::time::timeout(TEST_TIMEOUT, run())
		.await
		.map_err(|_| anyhow!("the flow test did not finish within {TEST_TIMEOUT:?}"))?
}

async fn run() -> Result<(), anyhow::Error> {
	log::info!("Spawning network");
	let config = build_network_config(NetworkOptions {
		validator_collators: true,
		election: true,
		hrmp_channel_max_total_size: None,
		sudo_balance: None,
		report_period: Some(REPORT_PERIOD),
		expected_era_duration: Some(EXPECTED_ERA_DURATION_MILLIS),
	})
	.await?;
	let network = initialize_network(config).await?;
	network.wait_until_is_up(120).await?;

	let relay_client: OnlineClient<PolkadotConfig> =
		network.get_node("alice")?.wait_client_with_timeout(CLIENT_TIMEOUT_SECS).await?;
	let asset_hub_client: OnlineClient<PolkadotConfig> = network
		.get_node(ASSET_HUB_INVULNERABLE)?
		.wait_client_with_timeout(CLIENT_TIMEOUT_SECS)
		.await?;
	let people_client: OnlineClient<PolkadotConfig> = network
		.get_node(PEOPLE_INVULNERABLE)?
		.wait_client_with_timeout(CLIENT_TIMEOUT_SECS)
		.await?;

	open_hrmp_channels(&relay_client, HRMP_MAX_MESSAGE_SIZE).await?;

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
	let asset_hub_early_report =
		tokio::spawn(wait_for_early_report(asset_hub_client.clone(), "Asset Hub"));
	asset_hub_announced.await??;
	log::info!("Waiting for People to receive the set of era 1");
	let people_received_at =
		tokio::time::timeout(PEOPLE_RECEIVE_TIMEOUT, people_received).await.map_err(
			|_| anyhow!("People did not receive the set within {PEOPLE_RECEIVE_TIMEOUT:?}"),
		)???;
	let people_early_report = tokio::spawn(wait_for_early_report(people_client.clone(), "People"));

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

	// AND Asset Hub credits the blocks authored on both parachains to exposed validators
	log::info!("Waiting for Asset Hub to credit the blocks of both parachains");
	wait_for_credits_from_both_chains(&asset_hub_client).await?;

	// AND both parachains report once before the expected end of era 1
	log::info!("Waiting for both parachains to report before the expected end of era 1");
	asset_hub_early_report.await??;
	people_early_report.await??;

	// AND the relay chain kept its validators, since the elected set is the running one
	let relay_validators = session_validators(&relay_client).await?;
	if relay_validators != stashes.iter().copied().collect::<BTreeSet<_>>() {
		return Err(anyhow!("relay chain session validators changed: {relay_validators:?}"));
	}

	log::info!("Test finished successfully");
	Ok(())
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

/// `ValidatorSetReceived { era: 1, count: 2 }`.
fn is_set_of_era_1_with_two_validators(fields: &Composite<u32>) -> bool {
	fields.at("era").and_then(|era| era.as_u128()) == Some(1) &&
		fields.at("count").and_then(|count| count.as_u128()) == Some(VALIDATORS.len() as u128)
}

/// Waits until Asset Hub credited the blocks of Asset Hub and of People, each to at least one
/// validator exposed in the active era.
///
/// Staking reports `ServicePointsCredited` for every credit, just before
/// `StakingPointsConverter` reports the source in `BlocksCredited`.
async fn wait_for_credits_from_both_chains(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	let wait = async {
		let mut pending = BTreeSet::from([ASSET_HUB_ID as u128, PEOPLE_ID as u128]);
		let mut blocks = client.blocks().subscribe_finalized().await?;
		while let Some(block) = blocks.next().await {
			let block = block?;
			let mut credited = 0;
			for event in block.events().await?.iter() {
				let event = event?;
				match (event.pallet_name(), event.variant_name()) {
					("Staking", "ServicePointsCredited") => {
						credited = event
							.field_values()?
							.at("credited")
							.and_then(|credited| credited.as_u128())
							.unwrap_or(0);
					},
					("StakingPointsConverter", "BlocksCredited") => {
						let fields = event.field_values()?;
						if let Some(source) = credit_source(&fields).filter(|_| credited > 0) {
							log::info!(
								"Asset Hub #{}: credited the blocks of {source}",
								block.number()
							);
							pending.remove(&source);
						}
						credited = 0;
					},
					_ => {},
				}
			}
			if pending.is_empty() {
				return Ok(());
			}
		}
		Err(anyhow!("Asset Hub block subscription ended"))
	};
	tokio::time::timeout(CREDIT_TIMEOUT, wait).await.map_err(|_| {
		anyhow!("Asset Hub did not credit the blocks of both chains within {CREDIT_TIMEOUT:?}")
	})?
}

/// Waits for the report before the expected era end on `chain`: the first finalized block that
/// removes `ValidatorCollators::SetReceivedAt`. A new arrival that replaces it does not count. The
/// block must hold `ValidatorCollators::BlocksReported`, since periodic reports alone emit it too.
async fn wait_for_early_report(
	client: OnlineClient<PolkadotConfig>,
	chain: &'static str,
) -> Result<u32, anyhow::Error> {
	let wait = async {
		let mut blocks = client.blocks().subscribe_finalized().await?;
		let mut arrival_stored = false;
		while let Some(block) = blocks.next().await {
			let block = block?;
			let stored =
				fetch_raw_at(&client, block.hash(), "ValidatorCollators", "SetReceivedAt", vec![])
					.await?
					.is_some();
			if arrival_stored && !stored {
				let reported = block.events().await?.iter().any(|event| {
					event.is_ok_and(|event| {
						event.pallet_name() == "ValidatorCollators" &&
							event.variant_name() == "BlocksReported"
					})
				});
				if !reported {
					return Err(anyhow!(
						"{chain} #{} removed `SetReceivedAt` without `BlocksReported`",
						block.number()
					));
				}
				log::info!("{chain} #{}: reported before the expected era end", block.number());
				return Ok(block.number());
			}
			arrival_stored = stored;
		}
		Err(anyhow!("{chain} block subscription ended"))
	};
	tokio::time::timeout(EARLY_REPORT_TIMEOUT, wait).await.map_err(|_| {
		anyhow!("{chain} did not report before the era end within {EARLY_REPORT_TIMEOUT:?}")
	})?
}

/// The parachain id in the `source` field of `BlocksCredited`.
fn credit_source(fields: &Composite<u32>) -> Option<u128> {
	let source = fields.at("source")?;
	source.as_u128().or_else(|| source.at(0).and_then(|id| id.as_u128()))
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
