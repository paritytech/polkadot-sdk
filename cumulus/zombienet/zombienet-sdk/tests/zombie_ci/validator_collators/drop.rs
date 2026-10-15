// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! People drops a validator collator that has session keys but runs no collator node, so authors
//! no blocks.
//!
//! Setup:
//! * `westend-local` with validators `alice` and `bob`.
//! * `people-westend-local` with one invulnerable collator.
//! * The relay chain calls People with `sudo` and an XCM `Transact`, which People dispatches as
//!   root.
//!
//! Test flow:
//! 1. In one call People gets session keys for a generated account with no node, a drop rule of two
//!    rounds, and a validator set holding only that account. The two forced rotations that follow
//!    the set enact the invulnerable and the account as Aura authorities, and the account's
//!    activity record starts at the enacting block.
//! 2. With half of the Aura slots empty, People keeps finalizing blocks until the window of two
//!    rounds of both authorities has passed since the enacting block.
//! 3. Setting no cap, which forces two rotations while a set is stored, drops the account at the
//!    first rotation and enacts the invulnerable alone at the second.

use super::common::{
	account, build_network_config, current_index, fetch_at, init_logging, keypair, node_seed,
	session_key_items, transact_as_root, wait_for_authorities_change, wait_for_event,
	wait_for_finalized_block, Chain, NetworkOptions, RotationBlock, CALL_TIMEOUT,
	CLIENT_TIMEOUT_SECS, PARA_BLOCKS, PARA_BLOCKS_TIMEOUT, PEOPLE_ID, PEOPLE_INVULNERABLE,
};
use crate::utils::initialize_network;

use anyhow::anyhow;
use codec::Decode;
use std::{collections::BTreeSet, time::Duration};
use tokio::time::Instant;
use zombienet_sdk::subxt::{
	self,
	dynamic::Value,
	ext::scale_value::{At, Composite, ValueDef},
	OnlineClient, PolkadotConfig,
};

/// The generated account in the validator set, with session keys on People and no node.
const IDLE_ACCOUNT_URI: &str = "//drop//idle";

/// Full Aura rounds the account may go without authoring.
const DROP_ROUNDS: u32 = 2;

/// Blocks the account is not returned, counted from the rotation that enacts the drop.
const DROP_BLOCKS: u32 = 2;

/// From the call that stores the set being finalized on the relay chain to People finalizing the
/// block that enacts the account: the relay chain delivers the message, then People rotates twice.
const SET_ENACTED_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// Slowest average block time while the account holds half of the Aura slots: the invulnerable
/// authors at least one block per relay chain block in its own slots and none in the account's.
const SLOWEST_BLOCK_TIME: Duration = Duration::from_secs(12);

/// Added to the window at `SLOWEST_BLOCK_TIME` for finality lag, from the block that enacts the
/// account to People finalizing the end of the window.
const WINDOW_MARGIN: Duration = Duration::from_secs(2 * 60);

/// From the call that sets no cap being finalized on the relay chain to People finalizing the
/// block that enacts the invulnerable alone: the relay chain delivers the message, then People
/// rotates twice.
const DROP_ENACTED_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// The whole test, below the 60-minute CI step so that the step ends with this error and the log
/// collection steps run. It covers the sum of the waits at their timeouts.
const TEST_TIMEOUT: Duration = Duration::from_secs(40 * 60);

/// `pallet_validator_collators::Activity` with People's block number.
#[derive(Debug, PartialEq, Decode)]
enum Activity {
	Queued,
	Active { since: u32, authorities: u32 },
}

#[tokio::test(flavor = "multi_thread")]
async fn validator_with_keys_and_no_node_is_dropped_on_people() -> Result<(), anyhow::Error> {
	init_logging();

	tokio::time::timeout(TEST_TIMEOUT, run())
		.await
		.map_err(|_| anyhow!("the drop test did not finish within {TEST_TIMEOUT:?}"))?
}

async fn run() -> Result<(), anyhow::Error> {
	log::info!("Spawning network");
	let config = build_network_config(NetworkOptions {
		asset_hub: false,
		validator_collators: false,
		election: false,
		hrmp_channel_max_total_size: None,
		sudo_balance: None,
		report_period: None,
		expected_era_duration: None,
	})
	.await?;
	let network = initialize_network(config).await?;
	network.wait_until_is_up(120).await?;

	let relay_client: OnlineClient<PolkadotConfig> =
		network.get_node("alice")?.wait_client_with_timeout(CLIENT_TIMEOUT_SECS).await?;
	let people_client: OnlineClient<PolkadotConfig> = network
		.get_node(PEOPLE_INVULNERABLE)?
		.wait_client_with_timeout(CLIENT_TIMEOUT_SECS)
		.await?;
	let people = Chain {
		name: "People",
		id: PEOPLE_ID,
		client: &people_client,
		invulnerable: PEOPLE_INVULNERABLE,
	};
	wait_for_finalized_block(&people_client, people.name, PARA_BLOCKS, PARA_BLOCKS_TIMEOUT).await?;

	let idle = account(&keypair(IDLE_ACCOUNT_URI)?);
	let invulnerable = account(&keypair(&node_seed(people.invulnerable))?);
	let max_blocks_per_slot: u32 = people_client
		.constants()
		.at(&subxt::dynamic::constant("ValidatorCollators", "MaxBlocksPerSlot"))?
		.as_type()?;

	// GIVEN the account with session keys and a drop rule on People
	// WHEN a validator set holding only the account arrives
	let index_before_set = current_index(&people_client).await?;
	let set_received = tokio::spawn(wait_for_event(
		people_client.clone(),
		people.name,
		"ValidatorCollators",
		"ValidatorSetReceived",
		is_set_of_one_validator,
		SET_ENACTED_TIMEOUT + CALL_TIMEOUT,
	));
	// The finalized block streams start at the current head, and the rotations come several
	// blocks after the call.
	let set_enacted = tokio::spawn(wait_for_authorities_change(
		people_client.clone(),
		people.name,
		SET_ENACTED_TIMEOUT + CALL_TIMEOUT,
	));
	store_rule_and_set(&relay_client, &people, idle).await?;
	set_received.await??;

	// THEN the next two rotations enact the account next to the invulnerable, and its activity
	// record starts at the enacting block
	let enacted = set_enacted.await??;
	check_authorities(&people, &enacted, &[invulnerable, idle], index_before_set + 2).await?;
	let activity: Option<Activity> = fetch_at(
		&people_client,
		enacted.hash,
		"ValidatorCollators",
		"LastActivity",
		vec![Value::from_bytes(idle)],
	)
	.await?;
	let expected = Activity::Active { since: enacted.number, authorities: 0 };
	if activity.as_ref() != Some(&expected) {
		return Err(anyhow!("activity of the account is {activity:?}, expected {expected:?}"));
	}

	// GIVEN the account authors nothing for a window after its enactment, while the invulnerable
	// keeps the chain going
	let window = DROP_ROUNDS * enacted.authorities as u32 * max_blocks_per_slot;
	let window_timeout = SLOWEST_BLOCK_TIME * window + WINDOW_MARGIN;
	let window_start = Instant::now();
	let window_end = enacted.number + window;
	wait_for_finalized_block(&people_client, people.name, window_end, window_timeout).await?;
	log::info!(
		"People finalized the window of {window} blocks in {:?} with half of the slots empty",
		window_start.elapsed()
	);

	// WHEN setting no cap forces two rotations
	let index_before_drop = current_index(&people_client).await?;
	let dropped_event = tokio::spawn(wait_for_event(
		people_client.clone(),
		people.name,
		"ValidatorCollators",
		"CollatorsDropped",
		is_drop_of_one_validator,
		DROP_ENACTED_TIMEOUT + CALL_TIMEOUT,
	));
	let drop_enacted = tokio::spawn(wait_for_authorities_change(
		people_client.clone(),
		people.name,
		DROP_ENACTED_TIMEOUT + CALL_TIMEOUT,
	));
	transact_as_root(
		&relay_client,
		&people,
		subxt::dynamic::tx(
			"ValidatorCollators",
			"set_max_collators",
			vec![Value::unnamed_variant("None", [])],
		),
	)
	.await?;

	// THEN the first rotation drops the account and the second enacts the invulnerable alone
	let dropped_at = dropped_event.await??;
	let drop_enacted = drop_enacted.await??;
	if dropped_at + 1 != drop_enacted.number {
		return Err(anyhow!(
			"the account was dropped at #{dropped_at}, not in the block before #{}",
			drop_enacted.number
		));
	}
	check_authorities(&people, &drop_enacted, &[invulnerable], index_before_drop + 2).await?;
	// AND the second rotation starts the drop of `DROP_BLOCKS` blocks, and the activity record of
	// the account is gone
	let dropped: Vec<([u8; 32], Option<u32>)> =
		fetch_at(&people_client, drop_enacted.hash, "ValidatorCollators", "Dropped", vec![])
			.await?
			.unwrap_or_default();
	let until = Some(drop_enacted.number + DROP_BLOCKS);
	if dropped != vec![(idle, until)] {
		return Err(anyhow!("drop records are {dropped:?}, expected the account until {until:?}"));
	}
	let activity: Option<Activity> = fetch_at(
		&people_client,
		drop_enacted.hash,
		"ValidatorCollators",
		"LastActivity",
		vec![Value::from_bytes(idle)],
	)
	.await?;
	if activity.is_some() {
		return Err(anyhow!("the dropped account still has the activity record {activity:?}"));
	}
	// AND People keeps finalizing blocks with the invulnerable alone
	wait_for_finalized_block(
		&people_client,
		people.name,
		drop_enacted.number + PARA_BLOCKS,
		PARA_BLOCKS_TIMEOUT,
	)
	.await?;

	log::info!("Test finished successfully");
	Ok(())
}

/// Through one call from the relay chain: session keys for `idle` whose own public key is its
/// Aura key, the drop rule, then a validator set of era 1 holding only `idle`.
async fn store_rule_and_set(
	relay_client: &OnlineClient<PolkadotConfig>,
	people: &Chain<'_>,
	idle: [u8; 32],
) -> Result<(), anyhow::Error> {
	let keys = session_key_items(people.client, &[idle])?
		.into_iter()
		.map(|(key, value)| {
			Value::unnamed_composite([Value::from_bytes(key), Value::from_bytes(value)])
		})
		.collect::<Vec<_>>();
	let rule = Value::named_composite([
		("rounds", Value::u128(DROP_ROUNDS.into())),
		("blocks", Value::u128(DROP_BLOCKS.into())),
		("min_collators", Value::u128(0)),
	]);
	let calls = [
		Value::unnamed_variant(
			"System",
			[Value::named_variant("set_storage", [("items", Value::unnamed_composite(keys))])],
		),
		Value::unnamed_variant(
			"ValidatorCollators",
			[Value::named_variant(
				"set_drop_rule",
				[("rule", Value::unnamed_variant("Some", [rule]))],
			)],
		),
		Value::unnamed_variant(
			"ValidatorCollators",
			[Value::named_variant(
				"set_validators",
				[
					("era", Value::u128(1)),
					("validators", Value::unnamed_composite([Value::from_bytes(idle)])),
				],
			)],
		),
	];
	transact_as_root(
		relay_client,
		people,
		subxt::dynamic::tx("Utility", "batch_all", vec![Value::unnamed_composite(calls)]),
	)
	.await
}

/// Checks that `rotation` enacts `expected` as Aura authorities and session validators, in
/// session `index`.
async fn check_authorities(
	chain: &Chain<'_>,
	rotation: &RotationBlock,
	expected: &[[u8; 32]],
	index: u32,
) -> Result<(), anyhow::Error> {
	let expected = expected.iter().copied().collect::<BTreeSet<_>>();
	let accounts = |pallet: &'static str, item: &'static str| async move {
		let accounts: Vec<[u8; 32]> = fetch_at(chain.client, rotation.hash, pallet, item, vec![])
			.await?
			.unwrap_or_default();
		Ok::<_, anyhow::Error>(accounts.into_iter().collect::<BTreeSet<_>>())
	};
	let authorities = accounts("Aura", "Authorities").await?;
	let validators = accounts("Session", "Validators").await?;
	let current: u32 = fetch_at(chain.client, rotation.hash, "Session", "CurrentIndex", vec![])
		.await?
		.unwrap_or_default();
	log::info!(
		"{} #{}: {} Aura authorities, {} session validators, session {current}",
		chain.name,
		rotation.number,
		authorities.len(),
		validators.len()
	);
	if authorities != expected || validators != expected || current != index {
		return Err(anyhow!(
			"{} #{} has authorities {authorities:?} and validators {validators:?} in session \
			 {current}, expected {expected:?} in session {index}",
			chain.name,
			rotation.number
		));
	}
	Ok(())
}

/// `ValidatorSetReceived { era: 1, count: 1 }`.
fn is_set_of_one_validator(fields: &Composite<u32>) -> bool {
	fields.at("era").and_then(|era| era.as_u128()) == Some(1) &&
		fields.at("count").and_then(|count| count.as_u128()) == Some(1)
}

/// `CollatorsDropped` with one validator and none kept. The set holds only the account, so the
/// validator is the account, which the drop records confirm.
fn is_drop_of_one_validator(fields: &Composite<u32>) -> bool {
	let one_dropped = fields.at("validators").is_some_and(
		|validators| matches!(&validators.value, ValueDef::Composite(c) if c.len() == 1),
	);
	one_dropped && fields.at("kept").and_then(|kept| kept.as_u128()) == Some(0)
}
