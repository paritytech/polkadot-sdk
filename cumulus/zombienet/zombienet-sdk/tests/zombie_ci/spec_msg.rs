// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Speculative messaging end to end: two penpal chains open a channel and deliver an XCM with no
//! HRMP channel between them.
//!
//! Each chain both sends and receives: the sender reads the receiver's `Ack` register, so each
//! collator names the other as its static `--spec-msg-peer`. Zombienet derives a node's key from
//! its name, so both peer ids are known before spawn.

use crate::utils::initialize_network;
use anyhow::anyhow;
use codec::Decode;
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{ChannelId, InChannelState, OutChannelState};
use cumulus_zombienet_sdk_helpers::{
	assert_para_throughput, open_hrmp_channel, submit_extrinsic_and_wait_for_finalization_success,
};
use std::{collections::BTreeMap, future::Future, time::Duration};
use zombienet_sdk::{
	subxt::{
		self,
		blocks::Block,
		dynamic::Value,
		ext::scale_value::{At, Composite, Primitive, ValueDef},
		utils::H256,
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfig, NetworkConfigBuilder,
};

const SENDER: u32 = 2000;
const RECEIVER: u32 = 2001;
const SENDER_COLLATOR: &str = "charlie";
const RECEIVER_COLLATOR: &str = "dave";
const SENDER_P2P_PORT: u16 = 30_500;
const RECEIVER_P2P_PORT: u16 = 30_501;

/// The XCM channel: domain 0, number 0.
fn channel(peer: u32) -> ChannelId {
	ChannelId { peer: peer.into(), domain: 0, num: 0 }
}

#[tokio::test(flavor = "multi_thread")]
async fn spec_msg_delivers_xcm_without_hrmp() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = initialize_network(build_network_config()?).await?;
	let relay: OnlineClient<PolkadotConfig> = network.get_node("alice")?.wait_client().await?;
	let sender: OnlineClient<PolkadotConfig> =
		network.get_node(SENDER_COLLATOR)?.wait_client().await?;
	let receiver: OnlineClient<PolkadotConfig> =
		network.get_node(RECEIVER_COLLATOR)?.wait_client().await?;

	log::info!("Waiting for both chains to produce blocks");
	assert_para_throughput(
		&relay,
		10,
		[(ParaId::from(SENDER), 5..11), (ParaId::from(RECEIVER), 5..11)],
		[],
	)
	.await?;

	open_spec_msg_channel(&sender, &receiver).await?;

	log::info!("Sending an XCM {SENDER} -> {RECEIVER}");
	sudo(&sender, send_xcm_to(RECEIVER)).await?;

	// The XCM reaches the receiver's message queue under `SpecMsg(sender)` and executes.
	wait_for_event(&receiver, "MessageQueue", "Processed", |fields| {
		let fields = format!("{fields:?}");
		fields.contains("SpecMsg") && fields.contains("Bool(true)")
	})
	.await?;

	// Both chains still make progress with messaging live.
	assert_para_throughput(
		&relay,
		10,
		[(ParaId::from(SENDER), 5..11), (ParaId::from(RECEIVER), 5..11)],
		[],
	)
	.await?;

	Ok(())
}

/// Samples per path.
const SAMPLES: usize = 10;

/// Samples per path: `SPEC_MSG_SAMPLES`, or [`SAMPLES`].
fn samples() -> usize {
	std::env::var("SPEC_MSG_SAMPLES")
		.ok()
		.and_then(|n| n.parse().ok())
		.unwrap_or(SAMPLES)
}

/// XCM delivery latency `SENDER -> RECEIVER`, spec-msg against HRMP, on one network.
///
/// Phase 1 sends over the spec-msg channel. Phase 2 force-opens an HRMP channel; the router then
/// prefers HRMP, so the same send goes over XCMP. Each phase sends one unmeasured message to
/// confirm the path, then [`SAMPLES`] messages one at a time, each processed before the next is
/// sent. A sample is the time from the sender block holding `PolkadotXcm::Sent` to the receiver
/// block holding the matching `MessageQueue::Processed`, by `Timestamp::Now` and by relay parent.
///
/// A measurement, not a check: run it by hand with `--ignored`.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn spec_msg_vs_hrmp_latency() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = initialize_network(build_network_config()?).await?;
	let relay: OnlineClient<PolkadotConfig> = network.get_node("alice")?.wait_client().await?;
	let sender: OnlineClient<PolkadotConfig> =
		network.get_node(SENDER_COLLATOR)?.wait_client().await?;
	let receiver: OnlineClient<PolkadotConfig> =
		network.get_node(RECEIVER_COLLATOR)?.wait_client().await?;
	assert_para_throughput(
		&relay,
		10,
		[(ParaId::from(SENDER), 5..11), (ParaId::from(RECEIVER), 5..11)],
		[],
	)
	.await?;

	let inclusions = track_inclusions(relay.clone());
	open_spec_msg_channel(&sender, &receiver).await?;
	let spec_msg = measure(&sender, &receiver, &inclusions, "SpecMsg").await?;

	log::info!("Force-opening HRMP {SENDER} -> {RECEIVER}");
	open_hrmp_channel(&relay, SENDER, RECEIVER, 8, 1024, &dev::alice(), 120).await?;
	wait_for("the HRMP channel to open on the relay chain", || async {
		let key = Value::named_composite([
			("sender", Value::u128(SENDER.into())),
			("recipient", Value::u128(RECEIVER.into())),
		]);
		let query = subxt::dynamic::storage("Hrmp", "HrmpChannels", vec![key]);
		Ok(relay.storage().at_latest().await?.fetch(&query).await?.is_some())
	})
	.await?;
	let hrmp = measure(&sender, &receiver, &inclusions, "Sibling").await?;

	report("spec-msg", &spec_msg);
	report("HRMP", &hrmp);
	Ok(())
}

/// One delivery: milliseconds and relay blocks from send to processing.
#[derive(Clone, Copy, Debug)]
struct Sample {
	millis: u64,
	relay_blocks: u32,
	/// Relay parent of the send block.
	sent_relay: u32,
	/// Relay block that included the send block.
	included_relay: u32,
	/// Relay parent of the block that processed the message.
	processed_relay: u32,
	sent_block: u32,
	processed_block: u32,
}

/// Relay block number of each included parachain head, by head hash.
type Inclusions = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<[u8; 32], u32>>>;

/// Record every `ParaInclusion::CandidateIncluded` on the relay chain's finalized blocks.
fn track_inclusions(relay: OnlineClient<PolkadotConfig>) -> Inclusions {
	let inclusions = Inclusions::default();
	let map = inclusions.clone();
	tokio::spawn(async move {
		let Ok(mut blocks) = relay.blocks().subscribe_finalized().await else { return };
		while let Some(Ok(block)) = blocks.next().await {
			let Ok(events) = block.events().await else { continue };
			for event in events.iter().flatten() {
				if event.pallet_name() != "ParaInclusion" ||
					event.variant_name() != "CandidateIncluded"
				{
					continue;
				}
				let Ok(fields) = event.field_values() else { continue };
				if let Some(head) = fields.at(1) {
					let hash = sp_crypto_hashing::blake2_256(&flatten(head));
					map.lock().unwrap().insert(hash, block.number());
				}
			}
		}
	});
	inclusions
}

/// The relay block that included `head`, waiting up to a minute for it to finalize.
async fn included_at(inclusions: &Inclusions, head: H256) -> Result<u32, anyhow::Error> {
	for _ in 0..60 {
		if let Some(number) = inclusions.lock().unwrap().get(&head.0) {
			return Ok(*number);
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
	Err(anyhow!("no inclusion of {head:?} seen"))
}

/// Send one warm-up message that must arrive under `origin`, then [`SAMPLES`] measured ones.
async fn measure(
	sender: &OnlineClient<PolkadotConfig>,
	receiver: &OnlineClient<PolkadotConfig>,
	inclusions: &Inclusions,
	origin: &str,
) -> Result<Vec<Sample>, anyhow::Error> {
	// The path can lag the channel state by a block or two: retry the warm-up until it arrives
	// under the expected origin.
	let mut warmed_up = false;
	for attempt in 1..=5 {
		let sample = deliver(sender, receiver, inclusions).await?;
		if sample.1 == origin {
			warmed_up = true;
			break;
		}
		log::info!("Warm-up {attempt} arrived under {}, expected {origin}", sample.1);
	}
	if !warmed_up {
		return Err(anyhow!("messages never arrived under {origin}"));
	}
	let mut samples = Vec::with_capacity(self::samples());
	for i in 1..=self::samples() {
		let (sample, arrived) = deliver(sender, receiver, inclusions).await?;
		if arrived != origin {
			return Err(anyhow!("sample {i} arrived under {arrived}, expected {origin}"));
		}
		log::info!(
			"{origin} sample {i}: {} ms, {} relay blocks; sent #{} (relay parent #{}), included at \
			 relay #{}, processed #{} (relay parent #{})",
			sample.millis,
			sample.relay_blocks,
			sample.sent_block,
			sample.sent_relay,
			sample.included_relay,
			sample.processed_block,
			sample.processed_relay,
		);
		samples.push(sample);
	}
	Ok(samples)
}

/// Send one XCM and wait for it to be processed. Returns the sample and the origin it arrived
/// under.
async fn deliver(
	sender: &OnlineClient<PolkadotConfig>,
	receiver: &OnlineClient<PolkadotConfig>,
	inclusions: &Inclusions,
) -> Result<(Sample, String), anyhow::Error> {
	// Subscribe first: the message may be processed before the send is finalized.
	let mut arrivals = receiver.blocks().subscribe_finalized().await?;
	let sent_in = sender.blocks().at(sudo(sender, send_xcm_to(RECEIVER)).await?).await?;
	let id = event_field(&sent_in, "PolkadotXcm", "Sent", "message_id")
		.await?
		.map(|id| flatten(&id))
		.ok_or_else(|| anyhow!("no `PolkadotXcm::Sent` in the send block"))?;
	let (sent_at, sent_relay) = when(&sent_in).await?;

	let wait = async {
		while let Some(block) = arrivals.next().await {
			let block = block?;
			for event in block.events().await?.iter() {
				let event = event?;
				if event.pallet_name() != "MessageQueue" || event.variant_name() != "Processed" {
					continue;
				}
				let fields = event.field_values()?;
				if fields.at("id").map(flatten) != Some(id.clone()) {
					continue;
				}
				if format!("{fields:?}").contains("Bool(false)") {
					return Err(anyhow!("message processed but failed: {fields:?}"));
				}
				let origin = match fields.at("origin").map(|origin| &origin.value) {
					Some(ValueDef::Variant(variant)) => variant.name.clone(),
					_ => return Err(anyhow!("no origin in {fields:?}")),
				};
				let (processed_at, processed_relay) = when(&block).await?;
				let sample = Sample {
					millis: processed_at.saturating_sub(sent_at),
					relay_blocks: processed_relay.saturating_sub(sent_relay),
					sent_relay,
					included_relay: included_at(inclusions, sent_in.hash()).await?,
					processed_relay,
					sent_block: sent_in.number(),
					processed_block: block.number(),
				};
				return Ok((sample, origin));
			}
		}
		Err(anyhow!("receiver block subscription ended"))
	};
	tokio::time::timeout(Duration::from_secs(180), wait)
		.await
		.map_err(|_| anyhow!("message not processed within 3 minutes"))?
}

/// The first `pallet::variant` event's `field` in `block`.
async fn event_field(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	pallet: &str,
	variant: &str,
	field: &str,
) -> Result<Option<Value<u32>>, anyhow::Error> {
	for event in block.events().await?.iter() {
		let event = event?;
		if event.pallet_name() == pallet && event.variant_name() == variant {
			return Ok(event.field_values()?.at(field).cloned());
		}
	}
	Ok(None)
}

/// A block's `Timestamp::Now` and relay parent number.
async fn when(
	block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) -> Result<(u64, u32), anyhow::Error> {
	let storage = block.storage();
	let read = |pallet: &'static str, item: &'static str| {
		let storage = storage.clone();
		async move {
			let query = subxt::dynamic::storage(pallet, item, Vec::<Value>::new());
			storage
				.fetch(&query)
				.await?
				.map(|value| value.into_encoded())
				.ok_or_else(|| anyhow!("{pallet}::{item} is empty"))
		}
	};
	let now = u64::decode(&mut &read("Timestamp", "Now").await?[..])?;
	let relay = u32::decode(&mut &read("ParachainSystem", "LastRelayChainBlockNumber").await?[..])?;
	Ok((now, relay))
}

/// All integers in `value`, depth first, as bytes: a byte array or hash of any nesting.
fn flatten(value: &Value<u32>) -> Vec<u8> {
	match &value.value {
		ValueDef::Primitive(Primitive::U128(n)) => vec![*n as u8],
		ValueDef::Composite(composite) => composite.values().flat_map(flatten).collect(),
		_ => Vec::new(),
	}
}

/// Log min, median, p80 (nearest rank) and max of `samples`: the total by time and by relay
/// blocks, and the relay blocks split into sender inclusion (send relay parent to inclusion) and
/// receiver pickup (inclusion to the processing block's relay parent).
fn report(path: &str, samples: &[Sample]) {
	fn stats(name: &str, mut values: Vec<i64>) -> String {
		values.sort();
		let rank = |p: usize| values[(values.len() * p).div_ceil(100).max(1) - 1];
		format!(
			"{name} min {} p50 {} p80 {} max {} {values:?}",
			values[0],
			rank(50),
			rank(80),
			values[values.len() - 1]
		)
	}
	let of = |f: fn(&Sample) -> i64| samples.iter().map(f).collect::<Vec<_>>();
	log::info!("{path}: {}", stats("ms", of(|s| s.millis as i64)));
	log::info!("{path}: {}", stats("relay blocks", of(|s| s.relay_blocks.into())));
	log::info!(
		"{path}: {}",
		stats("inclusion", of(|s| s.included_relay as i64 - s.sent_relay as i64))
	);
	log::info!(
		"{path}: {}",
		stats("pickup", of(|s| s.processed_relay as i64 - s.included_relay as i64))
	);
}

/// Open the XCM channel `SENDER -> RECEIVER` and wait until it carries sends: the receiver has
/// consumed the open signal and the sender has read the receiver's register.
async fn open_spec_msg_channel(
	sender: &OnlineClient<PolkadotConfig>,
	receiver: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	log::info!("Opening channel {SENDER} -> {RECEIVER}");
	sudo(
		sender,
		Value::unnamed_variant(
			"SpecMessaging",
			[Value::named_variant(
				"open_channel",
				[
					("recipient", Value::u128(RECEIVER.into())),
					("domain", Value::u128(0)),
					("num", Value::u128(0)),
				],
			)],
		),
	)
	.await?;
	sudo(
		receiver,
		Value::unnamed_variant(
			"SpecMessaging",
			[Value::named_variant(
				"accept_open_channel",
				[
					("sender", Value::u128(SENDER.into())),
					("domain", Value::u128(0)),
					("num", Value::u128(0)),
				],
			)],
		),
	)
	.await?;

	// The receiver consumes the `OpenChannel` signal from the sender's data stream: a channel
	// payload fetched from the sender's collator, lifted to an included root.
	wait_for("the receiver to consume the open signal", || async {
		let channels: BTreeMap<ChannelId, InChannelState> =
			call(receiver, "SpecMsgApi_in_channels").await?;
		Ok(channels.get(&channel(SENDER)).is_some_and(|state| state.published.up_to.0 > 0))
	})
	.await?;

	// The sender reads the receiver's register from its `Ack` stream: an `Events` item fetched
	// from the receiver's collator. Its grant opens the channel for sends.
	wait_for("the sender to read the receiver's register", || async {
		let channels: BTreeMap<ChannelId, OutChannelState> =
			call(sender, "SpecMsgApi_out_channels").await?;
		Ok(channels.get(&channel(RECEIVER)).is_some_and(|state| state.register.is_some()))
	})
	.await?;
	Ok(())
}

/// `PolkadotXcm::send` of `[UnsubscribeVersion]` to sibling `para`. Penpal's barrier admits it
/// from any origin, and it executes without paying or sending anything back.
fn send_xcm_to(para: u32) -> Value {
	let dest = Value::unnamed_variant(
		"V5",
		[Value::named_composite([
			("parents", Value::u128(1)),
			(
				"interior",
				Value::unnamed_variant(
					"X1",
					[Value::unnamed_composite([Value::unnamed_variant(
						"Parachain",
						[Value::u128(para.into())],
					)])],
				),
			),
		])],
	);
	let message = Value::unnamed_variant(
		"V5",
		[Value::unnamed_composite([Value::unnamed_composite([Value::unnamed_variant(
			"UnsubscribeVersion",
			[],
		)])])],
	);
	Value::unnamed_variant(
		"PolkadotXcm",
		[Value::named_variant("send", [("dest", dest), ("message", message)])],
	)
}

/// Dispatch `call` as root with `Sudo::sudo`, signed by Alice, and check the inner result.
async fn sudo(client: &OnlineClient<PolkadotConfig>, call: Value) -> Result<H256, anyhow::Error> {
	let tx = subxt::dynamic::tx("Sudo", "sudo", vec![call]);
	let block_hash =
		submit_extrinsic_and_wait_for_finalization_success(client, &tx, &dev::alice()).await?;
	let events = client.blocks().at(block_hash).await?.events().await?;
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == "Sudo" && event.variant_name() == "Sudid" {
			let fields = format!("{:?}", event.field_values()?);
			return if fields.contains("Err") {
				Err(anyhow!("sudo dispatch failed: {fields}"))
			} else {
				Ok(block_hash)
			};
		}
	}
	Err(anyhow!("no `Sudo::Sudid` in block {block_hash:?}"))
}

/// Call a runtime API with no arguments at the latest block and decode its result.
async fn call<T: Decode>(
	client: &OnlineClient<PolkadotConfig>,
	method: &str,
) -> Result<T, anyhow::Error> {
	let bytes = client.runtime_api().at_latest().await?.call_raw(method, None).await?;
	Ok(T::decode(&mut &bytes[..])?)
}

/// Poll `check` every 6 s for up to 5 minutes.
async fn wait_for<F, Fut>(what: &str, check: F) -> Result<(), anyhow::Error>
where
	F: Fn() -> Fut,
	Fut: Future<Output = Result<bool, anyhow::Error>>,
{
	log::info!("Waiting for {what}");
	for _ in 0..50 {
		if check().await? {
			log::info!("Done: {what}");
			return Ok(());
		}
		tokio::time::sleep(Duration::from_secs(6)).await;
	}
	Err(anyhow!("timed out waiting for {what}"))
}

/// Wait up to 5 minutes for a finalized `pallet::variant` event whose fields match.
async fn wait_for_event(
	client: &OnlineClient<PolkadotConfig>,
	pallet: &str,
	variant: &str,
	matches: impl Fn(&Composite<u32>) -> bool,
) -> Result<(), anyhow::Error> {
	log::info!("Waiting for `{pallet}::{variant}`");
	let wait = async {
		let mut blocks = client.blocks().subscribe_finalized().await?;
		while let Some(block) = blocks.next().await {
			let block = block?;
			for event in block.events().await?.iter() {
				let event = event?;
				if event.pallet_name() != pallet || event.variant_name() != variant {
					continue;
				}
				let fields = event.field_values()?;
				if matches(&fields) {
					log::info!("#{}: `{pallet}::{variant}` {fields:?}", block.number());
					return Ok(());
				}
			}
		}
		Err(anyhow!("block subscription ended"))
	};
	tokio::time::timeout(Duration::from_secs(300), wait)
		.await
		.map_err(|_| anyhow!("no matching `{pallet}::{variant}` within 5 minutes"))?
}

/// `--spec-msg-peer` for the collator `name` of `para`, listening on `port`.
fn peer_arg(para: u32, name: &str, port: u16) -> Result<String, anyhow::Error> {
	let (_, peer_id) = zombienet_orchestrator::generators::generate_node_identity(name)
		.map_err(|e| anyhow!("peer id of {name}: {e}"))?;
	Ok(format!("{para}=/ip4/127.0.0.1/tcp/{port}/ws/p2p/{peer_id}"))
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let to_receiver = peer_arg(RECEIVER, RECEIVER_COLLATOR, RECEIVER_P2P_PORT)?;
	let to_sender = peer_arg(SENDER, SENDER_COLLATOR, SENDER_P2P_PORT)?;
	let collator_args = |peer: &str| -> Vec<zombienet_sdk::Arg> {
		vec![
			("-lparachain=debug,spec-msg=debug,xcm=info,runtime::spec-messaging=debug").into(),
			("--force-authoring").into(),
			("--enable-spec-msg").into(),
			("--spec-msg-peer", peer).into(),
		]
	};

	// Distinct `protocolId`s: chains sharing one negotiate the legacy block-announce name, fail its
	// genesis check and ban each other, refusing every spec-msg fetch for about a minute.
	// Network setup:
	// - relay chain: alice and bob, validators
	// - penpal 2000: charlie, collator, fetching from dave
	// - penpal 2001: dave, collator, fetching from charlie
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_validator(|node| node.with_name("alice"))
				.with_validator(|node| node.with_name("bob"))
		})
		.with_parachain(|p| {
			p.with_id(SENDER)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain(format!("penpal-rococo-{SENDER}").as_str())
				.with_raw_spec_override(
					serde_json::json!({ "protocolId": format!("penpal-{SENDER}") }),
				)
				.with_default_args(collator_args(&to_receiver))
				.with_collator(|n| n.with_name(SENDER_COLLATOR).with_p2p_port(SENDER_P2P_PORT))
		})
		.with_parachain(|p| {
			p.with_id(RECEIVER)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain(format!("penpal-rococo-{RECEIVER}").as_str())
				.with_raw_spec_override(
					serde_json::json!({ "protocolId": format!("penpal-{RECEIVER}") }),
				)
				.with_default_args(collator_args(&to_sender))
				.with_collator(|n| n.with_name(RECEIVER_COLLATOR).with_p2p_port(RECEIVER_P2P_PORT))
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
