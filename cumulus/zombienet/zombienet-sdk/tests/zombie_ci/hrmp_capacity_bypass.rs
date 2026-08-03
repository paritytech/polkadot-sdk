// This file is part of Cumulus.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Regression test for the HRMP channel capacity bypass via asynchronous backing.
//!
//! `check_outbound_hrmp` used to validate a candidate against the *committed* `HrmpChannels`
//! state, which only updates on inclusion. Several candidates backed against the same
//! pre-update state each passed individually, then summed past `max_capacity` once enacted. The
//! fix checks each candidate against the running total of the para's unincluded segment.
//!
//! The bug is unreachable with honest software, so the test disables the layers above the
//! runtime: the collator self-limits in `adjust_egress_bandwidth_limits` (bypassed by
//! `TestPallet::set_oversend_hrmp`), and both backers and the block author consult
//! prospective-parachains, whose `check_modifications` enforces the same limit node-side
//! (bypassed by running every validator as `malus ignore-message-constraints`). That harness is
//! identical on master and on the fix, so the on-chain check is the only variable.
//!
//! Asserts `msg_count <= max_capacity` and `total_size <= max_total_size` at every relay block.

use crate::utils::initialize_network;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assign_cores, submit_extrinsic_and_wait_for_finalization_success,
};
use serde_json::json;
use zombienet_sdk::{
	subxt::{
		self,
		ext::scale_value::{value, At, Value},
		utils::H256,
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

/// The over-sending parachain, on the `elastic-scaling` spec so it can occupy several cores.
const SENDER_PARA_ID: u32 = 2100;
/// The recipient, on the `sync-backing` spec (12s slots). Its block rate is the channel's drain
/// rate — `hrmp::prune_hrmp` decrements `msg_count` when the recipient advances its watermark —
/// so a slow recipient keeps the counter elevated and the assertion sensitive.
const RECIPIENT_PARA_ID: u32 = 2500;

/// 2, not 1, so the fixed runtime still lets one candidate per segment through and the para keeps
/// producing. A vulnerable runtime overshoots it with three cores' worth of candidates.
const CHANNEL_MAX_CAPACITY: u32 = 2;
const CHANNEL_MAX_MESSAGE_SIZE: u32 = 128;

/// Must stay 1: `check_outbound_hrmp` requires strictly ascending recipients, so two messages to
/// the same recipient in one candidate are rejected as `NotSorted`. The overflow therefore comes
/// from summing across pipelined candidates, which is the reported attack anyway.
const OVERSEND_PER_BLOCK: u32 = 1;

/// Indices count over the cores zombienet did *not* auto-assign (it adds one per registered para),
/// so the sender ends up with these two plus its own.
const SENDER_CORES: [u32; 2] = [0, 1];

const OBSERVE_RELAY_BLOCKS: u32 = 24;

#[tokio::test(flavor = "multi_thread")]
async fn hrmp_channel_capacity_is_not_bypassed_by_async_backing() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let sender_node = network.get_node("collator-oversend")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let sender_client: OnlineClient<PolkadotConfig> = sender_node.wait_client().await?;

	// Give the sender several cores so that multiple of its candidates are backed against the
	// same relay parent and sit in the unincluded segment together.
	assign_cores(&relay_client, SENDER_PARA_ID, SENDER_CORES.to_vec()).await?;

	// Open the channel with a deliberately tiny capacity. This must happen *before* the parachain
	// starts over-sending, otherwise its candidates are rejected for `NoSuchChannel` and the para
	// stalls before it can reach the interesting state.
	log::info!(
		"Opening HRMP channel {SENDER_PARA_ID} -> {RECIPIENT_PARA_ID} \
		 (max_capacity={CHANNEL_MAX_CAPACITY}, max_message_size={CHANNEL_MAX_MESSAGE_SIZE})"
	);
	let open_channel = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Hrmp(force_open_hrmp_channel {
				sender: SENDER_PARA_ID,
				recipient: RECIPIENT_PARA_ID,
				max_capacity: CHANNEL_MAX_CAPACITY,
				max_message_size: CHANNEL_MAX_MESSAGE_SIZE
			})
		}],
	);
	submit_extrinsic_and_wait_for_finalization_success(&relay_client, &open_channel, &dev::alice())
		.await?;

	let before = wait_for_channel(&relay_client).await?;
	log::info!(
		"Channel open: msg_count={} total_size={} max_capacity={} max_total_size={}",
		before.msg_count,
		before.total_size,
		before.max_capacity,
		before.max_total_size
	);
	assert_eq!(before.max_capacity, CHANNEL_MAX_CAPACITY, "unexpected negotiated capacity");

	log::info!("Enabling over-sending of {OVERSEND_PER_BLOCK} HRMP msgs/block on the parachain");
	let enable_oversend = subxt::tx::dynamic(
		"TestPallet",
		"set_oversend_hrmp",
		vec![Value::u128(OVERSEND_PER_BLOCK as u128), Value::u128(RECIPIENT_PARA_ID as u128)],
	);
	submit_extrinsic_and_wait_for_finalization_success(
		&sender_client,
		&enable_oversend,
		&dev::alice(),
	)
	.await?;

	log::info!("Watching the channel for {OBSERVE_RELAY_BLOCKS} relay blocks");
	let mut blocks = relay_client.blocks().subscribe_best().await?;
	let mut observed = 0u32;
	let mut peak_msg_count = 0u32;
	let mut peak_total_size = 0u32;

	while let Some(block) = blocks.next().await {
		let block = block?;
		let Some(channel) = read_channel(&relay_client, block.hash()).await? else {
			continue;
		};

		peak_msg_count = peak_msg_count.max(channel.msg_count);
		peak_total_size = peak_total_size.max(channel.total_size);

		log::debug!(
			"Relay #{}: msg_count={}/{} total_size={}/{}",
			block.number(),
			channel.msg_count,
			channel.max_capacity,
			channel.total_size,
			channel.max_total_size,
		);

		assert!(
			channel.msg_count <= channel.max_capacity,
			"HRMP channel capacity bypassed at relay block #{}: msg_count {} exceeds \
			 max_capacity {} — pipelined candidates were each checked against the same committed \
			 channel state.",
			block.number(),
			channel.msg_count,
			channel.max_capacity,
		);
		assert!(
			channel.total_size <= channel.max_total_size,
			"HRMP channel total size bypassed at relay block #{}: total_size {} exceeds \
			 max_total_size {}.",
			block.number(),
			channel.total_size,
			channel.max_total_size,
		);

		observed += 1;
		if observed >= OBSERVE_RELAY_BLOCKS {
			break;
		}
	}

	if observed < OBSERVE_RELAY_BLOCKS {
		return Err(anyhow!(
			"Relay chain stopped producing blocks after {observed}/{OBSERVE_RELAY_BLOCKS}"
		));
	}

	log::info!(
		"Channel stayed within limits for {observed} relay blocks \
		 (peak msg_count={peak_msg_count}/{CHANNEL_MAX_CAPACITY}, peak total_size={peak_total_size})"
	);

	// Guard against a vacuous pass.
	assert!(
		peak_msg_count > 0,
		"No HRMP messages ever reached the channel — the parachain was not over-sending, so this \
		 run did not exercise the acceptance check at all."
	);

	log::info!("Test finished successfully");
	Ok(())
}

/// The fields of `polkadot_runtime_parachains::hrmp::HrmpChannel` this test cares about.
#[derive(Debug, Clone, Copy)]
struct ChannelState {
	max_capacity: u32,
	max_total_size: u32,
	msg_count: u32,
	total_size: u32,
}

/// Polls until the sender→recipient channel shows up in relay chain storage.
async fn wait_for_channel(
	relay_client: &OnlineClient<PolkadotConfig>,
) -> Result<ChannelState, anyhow::Error> {
	let mut blocks = relay_client.blocks().subscribe_best().await?;
	let mut waited = 0;

	while let Some(block) = blocks.next().await {
		let block = block?;
		if let Some(channel) = read_channel(relay_client, block.hash()).await? {
			return Ok(channel);
		}
		waited += 1;
		if waited > 30 {
			return Err(anyhow!(
				"HRMP channel {SENDER_PARA_ID} -> {RECIPIENT_PARA_ID} did not open within \
				 {waited} relay blocks"
			));
		}
	}

	Err(anyhow!("Relay chain stopped producing blocks while waiting for the HRMP channel"))
}

/// Reads the sender→recipient entry out of `Hrmp::HrmpChannels`. Iterates and filters rather than
/// fetching by key, to avoid hand-encoding the `HrmpChannelId` composite.
async fn read_channel(
	relay_client: &OnlineClient<PolkadotConfig>,
	block_hash: H256,
) -> Result<Option<ChannelState>, anyhow::Error> {
	let query = subxt::dynamic::storage("Hrmp", "HrmpChannels", Vec::<Value>::new());
	let mut entries = relay_client.storage().at(block_hash).iter(query).await?;

	while let Some(entry) = entries.next().await {
		let entry = entry?;

		let key = entry.keys.first().ok_or_else(|| anyhow!("HrmpChannels entry has no key"))?;
		let (sender, recipient) = (field_u32(key, "sender")?, field_u32(key, "recipient")?);
		if sender != SENDER_PARA_ID || recipient != RECIPIENT_PARA_ID {
			continue;
		}

		let value = entry.value.to_value()?;
		return Ok(Some(ChannelState {
			max_capacity: field_u32(&value, "max_capacity")?,
			max_total_size: field_u32(&value, "max_total_size")?,
			msg_count: field_u32(&value, "msg_count")?,
			total_size: field_u32(&value, "total_size")?,
		}));
	}

	Ok(None)
}

fn field_u32<T: std::fmt::Debug>(value: &Value<T>, name: &str) -> Result<u32, anyhow::Error> {
	let field = value.at(name).ok_or_else(|| anyhow!("field `{name}` missing from {value:?}"))?;
	as_u32(field).ok_or_else(|| anyhow!("field `{name}` is not a u32: {field:?}"))
}

/// Descends through newtype wrappers (`ParaId` is a single-field composite over `u32`).
fn as_u32<T>(value: &Value<T>) -> Option<u32> {
	if let Some(n) = value.as_u128() {
		return u32::try_from(n).ok();
	}
	as_u32(value.at(0)?)
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	let malus_image =
		std::env::var("MALUS_IMAGE").unwrap_or_else(|_| "docker.io/paritypr/malus".to_string());

	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				// Must stay `polkadot`: zombienet builds the chain spec by running the relaychain
				// default command with `build-spec --chain ...`, and `malus` takes its variant as a
				// leading subcommand, so it rejects a bare `--chain`. Validators override it below.
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// Becomes 6 with zombienet's one-per-para addition; matched by the
								// six validators, since group size 1 means an unbacked core is
								// a wasted core.
								"num_cores": 4,
								"max_validators_per_core": 1
							},
							// Ceiling only — the channel is opened far smaller, and that smaller
							// capacity is the limit under test.
							"hrmp_channel_max_capacity": 8,
							"hrmp_channel_max_total_size": 8192,
							"hrmp_channel_max_message_size": 1024,
							// Must exceed OVERSEND_PER_BLOCK, or candidates trip the per-candidate
							// limit instead of the cumulative one.
							"hrmp_max_message_num_per_candidate": 10,
							"hrmp_max_parachain_outbound_channels": 10,
							"hrmp_max_parachain_inbound_channels": 10
						}
					}
				}))
				// Outside the fold so that `r` has the right type.
				.with_validator(|node| {
					node.with_name("validator-0")
						.with_command("malus")
						.with_image(malus_image.as_str())
						.with_subcommand("ignore-message-constraints")
						.with_args(vec![
							("-lparachain=debug,MALUS=trace").into(),
							// Without this the malus validator won't run on macOS.
							("--insecure-validator-i-know-what-i-do").into(),
						])
				});

			// All validators are malicious: the backing group must collude *and* the block author
			// must be willing to put the candidate in the inherent.
			(1..6).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}"))
						.with_command("malus")
						.with_image(malus_image.as_str())
						.with_subcommand("ignore-message-constraints")
						.with_args(vec![
							("-lparachain=debug,MALUS=trace").into(),
							("--insecure-validator-i-know-what-i-do").into(),
						])
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(SENDER_PARA_ID)
				.with_chain("elastic-scaling")
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("collator-oversend").with_args(vec![
						("-lparachain=debug,aura=debug,runtime=info").into(),
						("--force-authoring").into(),
						("--authoring", "slot-based").into(),
					])
				})
		})
		.with_parachain(|p| {
			p.with_id(RECIPIENT_PARA_ID)
				.with_chain("sync-backing")
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| n.with_name("collator-recipient"))
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
