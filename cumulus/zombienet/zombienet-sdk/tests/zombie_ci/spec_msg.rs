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
	assert_para_throughput, submit_extrinsic_and_wait_for_finalization_success,
};
use std::{collections::BTreeMap, future::Future, time::Duration};
use zombienet_sdk::{
	subxt::{self, dynamic::Value, ext::scale_value::Composite, OnlineClient, PolkadotConfig},
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

	log::info!("Opening channel {SENDER} -> {RECEIVER}");
	sudo(
		&sender,
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
		&receiver,
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
			call(&receiver, "SpecMsgApi_in_channels").await?;
		Ok(channels.get(&channel(SENDER)).is_some_and(|state| state.published.up_to.0 > 0))
	})
	.await?;

	// The sender reads the receiver's register from its `Ack` stream: an `Events` item fetched
	// from the receiver's collator. Its grant opens the channel for sends.
	wait_for("the sender to read the receiver's register", || async {
		let channels: BTreeMap<ChannelId, OutChannelState> =
			call(&sender, "SpecMsgApi_out_channels").await?;
		Ok(channels.get(&channel(RECEIVER)).is_some_and(|state| state.register.is_some()))
	})
	.await?;

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
async fn sudo(client: &OnlineClient<PolkadotConfig>, call: Value) -> Result<(), anyhow::Error> {
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
				Ok(())
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
				.with_default_args(collator_args(&to_receiver))
				.with_collator(|n| n.with_name(SENDER_COLLATOR).with_p2p_port(SENDER_P2P_PORT))
		})
		.with_parachain(|p| {
			p.with_id(RECEIVER)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain(format!("penpal-rococo-{RECEIVER}").as_str())
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
