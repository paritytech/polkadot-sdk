// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Parachain-to-parachain XCM delivery over Speculative Messaging — the
//! end-to-end cutover test gating the HRMP→spec-msg rollout.
//!
//! Spawns a rococo-local relay chain and two penpal parachains whose
//! collators are wired for spec-msg (static `--spec-msg-source-peer`
//! addresses; zombienet derives node keys from node names, so the peer ids
//! are known up front), then walks the transition runbook:
//!
//! 1. **Baseline**: HRMP channels open in both directions; XCM A→B delivers over HRMP.
//! 2. **Arm spec-msg channels** in both directions (the MVP placeholder arming:
//!    `OpenOutboundChannels` + `ConsumedStreams` via sudo `set_storage`; the real
//!    open/accept/register handshake is the channels & flow-control issue) and assert nothing
//!    changes on the wire — the router still prefers the open HRMP channel.
//! 3. **Cutover**: set `HrmpClosing` on both sides (new traffic diverts, the pipe drains), verify
//!    the drain, close the HRMP channels relay-side.
//! 4. **Deliver over spec-msg** in both directions: the sender's stream frontier advances and its
//!    header carries the `SPMS` digest, the relay's `RecentProvides` ring gains the sender's root,
//!    and the receiver executes the message under the `SpecMsg(source)` origin — the
//!    Sibling-identical origin, asserted via `MessageQueue.Processed`.
//! 5. **Rollback**: re-open HRMP one direction and clear the flag; the router reverts to HRMP.
//! 6. **Unroutable**: a sibling with neither transport rejects the send.
//!
//! Delivery is exactly-once throughout: the final sweep tallies every
//! `MessageQueue.Processed` event by origin against the number of sends.

use anyhow::anyhow;
use codec::{Decode, Encode};
use std::{
	collections::HashMap,
	time::{Duration, Instant},
};

use cumulus_primitives_spec_messaging::{ChannelId, MmrFrontier, StreamId};
use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as RelayParaId;
use sp_core::crypto::AccountId32 as SpAccountId32;
use xcm::latest::{
	Junction as XcmJunction, Location as XcmLocation, NetworkId, WESTEND_GENESIS_HASH,
};
use xcm_builder::{DescribeAllTerminal, DescribeFamily, DescribeTerminus, HashedDescription};
use xcm_executor::traits::ConvertLocation;
use zombienet_sdk::{
	subxt::{
		config::substrate::DigestItem,
		utils::{AccountId32, MultiAddress},
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

use super::hrmp_penpal::wait_for_hrmp_drain;

#[zombienet_sdk::subxt::subxt(runtime_metadata_path = "metadata-files/rococo-local.scale")]
mod rococo {}

#[zombienet_sdk::subxt::subxt(runtime_metadata_path = "metadata-files/penpal-local.scale")]
mod penpal {}

use rococo::runtime_types::{
	pallet_utility::pallet::Call as UtilityCall,
	polkadot_parachain_primitives::primitives::{HrmpChannelId, Id},
	polkadot_runtime_parachains::hrmp::pallet::Call as HrmpCall,
	rococo_runtime::RuntimeCall as RococoRuntimeCall,
};

use penpal::runtime_types::{
	cumulus_pallet_spec_messaging::pallet::Call as SpecMessagingCall,
	cumulus_primitives_core::AggregateMessageOrigin,
	frame_system::pallet::Call as SystemCall,
	pallet_balances::pallet::Call as BalancesCall,
	penpal_runtime::RuntimeCall as PenpalRuntimeCall,
	polkadot_parachain_primitives::primitives::Id as PenpalId,
	staging_xcm::v5::{
		asset::{Asset, AssetFilter, AssetId, Assets, Fungibility, WildAsset},
		junction::Junction,
		junctions::Junctions,
		location::Location,
		Instruction, Xcm,
	},
	xcm::{v3::WeightLimit, VersionedLocation, VersionedXcm},
};

const PARA_A: u32 = 2000;
const PARA_B: u32 = 2001;
/// A sibling with neither an HRMP channel nor a spec-msg channel.
const PARA_NOWHERE: u32 = 2002;

/// Fixed parachain-side p2p ports, so each collator's multiaddr is known
/// when the *other* collator's args are rendered.
const PENPAL_A_P2P_PORT: u16 = 43210;
const PENPAL_B_P2P_PORT: u16 = 43310;

// HRMP channel parameters, well within rococo-local's host configuration limits.
const HRMP_MAX_CAPACITY: u32 = 4;
const HRMP_MAX_MESSAGE_SIZE: u32 = 524288;

// Penpal uses 12 decimals ("UNIT").
const UNIT: u128 = 1_000_000_000_000;
// Balance given to the sender's derived account on the destination.
const SENDER_FUNDS: u128 = 1_000 * UNIT;
// Amount withdrawn by each XCM program; covers fees, the rest is deposited.
const TRANSFER_AMOUNT: u128 = 10 * UNIT;

/// The `SPMS` header digest's consensus engine id (`SPMS_ENGINE_ID`).
const SPMS_ENGINE_ID: [u8; 4] = *b"SPMS";

/// The multiaddr (with peer id) of a zombienet parachain collator's
/// parachain-side network: zombienet listens on `/ip4/0.0.0.0/tcp/<port>/ws`
/// and derives the node key from the node's name.
fn collator_multiaddr(name: &str, p2p_port: u16) -> anyhow::Result<String> {
	let (_node_key, peer_id) = zombienet_orchestrator::generators::generate_node_identity(name)
		.map_err(|e| anyhow!("failed to derive {name}'s node identity: {e:?}"))?;
	Ok(format!("/ip4/127.0.0.1/tcp/{p2p_port}/ws/p2p/{peer_id}"))
}

/// The account on the destination penpal that an incoming XCM sent by
/// `sender` on the sibling `source_para` resolves to (see the HRMP baseline
/// test for the derivation).
fn sender_account_on_dest(source_para: u32, sender: [u8; 32]) -> anyhow::Result<SpAccountId32> {
	let origin_on_dest = XcmLocation::new(
		1,
		[
			XcmJunction::Parachain(source_para),
			XcmJunction::AccountId32 {
				network: Some(NetworkId::ByGenesis(WESTEND_GENESIS_HASH)),
				id: sender,
			},
		],
	);
	HashedDescription::<SpAccountId32, (DescribeTerminus, DescribeFamily<DescribeAllTerminal>)>::convert_location(
		&origin_on_dest,
	)
	.ok_or_else(|| anyhow!("failed to derive the sender's account on the destination"))
}

/// Raw storage key of `SpecMessaging::<item>` under a `Twox64Concat` map key.
fn spec_messaging_key(item: &str, map_key: &[u8]) -> Vec<u8> {
	let mut key = sp_crypto_hashing::twox_128(b"SpecMessaging").to_vec();
	key.extend(sp_crypto_hashing::twox_128(item.as_bytes()));
	key.extend(sp_crypto_hashing::twox_64(map_key));
	key.extend(map_key);
	key
}

/// The designated XCM channel stream `sender -> recipient`.
fn xcm_channel_stream(recipient: u32) -> StreamId {
	StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
}

/// Arms the placeholder spec-msg channel state on one penpal via sudo
/// `set_storage`: the outbound channel to `peer` and consumption of `peer`'s
/// designated XCM stream. Issue 15's open/accept handshake replaces this.
async fn arm_spec_msg_channel(
	client: &OnlineClient<PolkadotConfig>,
	own_para: u32,
	peer: u32,
) -> anyhow::Result<()> {
	let out_channel = ChannelId { peer: peer.into(), domain: 0, num: 0 };
	let consumed = (RelayParaId::from(peer), xcm_channel_stream(own_para));
	let items = vec![
		(spec_messaging_key("OpenOutboundChannels", &out_channel.encode()), ().encode()),
		(spec_messaging_key("ConsumedStreams", &consumed.encode()), ().encode()),
	];
	let arm_tx = penpal::tx()
		.sudo()
		.sudo(PenpalRuntimeCall::System(SystemCall::set_storage { items }));
	client
		.tx()
		.sign_and_submit_then_watch_default(&arm_tx, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(())
}

/// The sender-side outbound frontier of `stream`, if any leaf was appended.
async fn outbound_frontier(
	client: &OnlineClient<PolkadotConfig>,
	stream: &StreamId,
) -> anyhow::Result<Option<MmrFrontier>> {
	let key = spec_messaging_key("OutboundFrontier", &stream.encode());
	let raw = client.storage().at_latest().await?.fetch_raw(key).await?;
	raw.map(|bytes| {
		MmrFrontier::decode(&mut bytes.as_slice())
			.map_err(|e| anyhow!("undecodable OutboundFrontier: {e}"))
	})
	.transpose()
}

/// The relay's `SpecMsg::RecentProvides` ring of `sender`, oldest first.
async fn recent_provides(
	relay_client: &OnlineClient<PolkadotConfig>,
	sender: u32,
) -> anyhow::Result<Vec<(sp_core::H256, u32)>> {
	let key =
		polkadot_primitives::well_known_keys::spec_msg_recent_provides(RelayParaId::from(sender));
	let raw = relay_client.storage().at_latest().await?.fetch_raw(key).await?;
	Ok(raw
		.map(|bytes| Vec::<(sp_core::H256, u32)>::decode(&mut bytes.as_slice()))
		.transpose()?
		.unwrap_or_default())
}

/// Waits (on finalized blocks) for a successful `MessageQueue.Processed`
/// event whose origin is `SpecMsg(para)` (`spec_msg: true`) or
/// `Sibling(para)` (`spec_msg: false`).
async fn wait_for_message_processed(
	client: &OnlineClient<PolkadotConfig>,
	para: u32,
	spec_msg: bool,
) -> anyhow::Result<bool> {
	let mut blocks = client.blocks().subscribe_finalized().await?;
	while let Some(block) = blocks.next().await {
		let events = block?.events().await?;
		for event in events.find::<penpal::message_queue::events::Processed>() {
			let event = event?;
			let matches = match (&event.origin, spec_msg) {
				(AggregateMessageOrigin::SpecMsg(id), true) => id.0 == para,
				(AggregateMessageOrigin::Sibling(id), true) if id.0 == para => {
					return Err(anyhow!("message from {para} was processed under Sibling origin"))
				},
				(AggregateMessageOrigin::Sibling(id), false) => id.0 == para,
				_ => false,
			};
			if matches {
				return Ok(event.success);
			}
		}
	}
	Err(anyhow!("block subscription ended before the message was processed"))
}

/// A withdraw/pay/deposit XCM program for the destination's native token,
/// depositing to `receiver`.
fn transfer_program(receiver: [u8; 32]) -> VersionedXcm {
	let native = || Location { parents: 0, interior: Junctions::Here };
	VersionedXcm::V5(Xcm(vec![
		Instruction::WithdrawAsset(Assets(vec![Asset {
			id: AssetId(native()),
			fun: Fungibility::Fungible(TRANSFER_AMOUNT),
		}])),
		Instruction::BuyExecution {
			fees: Asset { id: AssetId(native()), fun: Fungibility::Fungible(TRANSFER_AMOUNT) },
			weight_limit: WeightLimit::Unlimited,
		},
		Instruction::DepositAsset {
			assets: AssetFilter::Wild(WildAsset::All),
			beneficiary: Location {
				parents: 0,
				interior: Junctions::X1([Junction::AccountId32 { network: None, id: receiver }]),
			},
		},
	]))
}

/// Sends `message` from `sender_client`'s penpal to sibling `dest_para`.
/// Asserts the send succeeded and — per `expect_hrmp` — which transport took
/// it (`XcmpQueue.XcmpMessageSent` is the HRMP transport's send event).
async fn send_xcm_to_sibling(
	sender_client: &OnlineClient<PolkadotConfig>,
	dest_para: u32,
	message: VersionedXcm,
	expect_hrmp: bool,
) -> anyhow::Result<()> {
	let dest = VersionedLocation::V5(Location {
		parents: 1,
		interior: Junctions::X1([Junction::Parachain(dest_para)]),
	});
	let send_tx = penpal::tx().polkadot_xcm().send(dest, message);
	let send_events = sender_client
		.tx()
		.sign_and_submit_then_watch_default(&send_tx, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	if !send_events.has::<penpal::polkadot_xcm::events::Sent>()? {
		return Err(anyhow!("no PolkadotXcm.Sent event on the sender"));
	}
	let over_hrmp = send_events.has::<penpal::xcmp_queue::events::XcmpMessageSent>()?;
	if over_hrmp != expect_hrmp {
		return Err(anyhow!(
			"message to {dest_para} went out over {} but {} was expected",
			if over_hrmp { "HRMP (XcmpQueue)" } else { "spec-msg" },
			if expect_hrmp { "HRMP (XcmpQueue)" } else { "spec-msg" },
		));
	}
	Ok(())
}

/// Funds, via sudo on the destination, the account that alice's sends from
/// `source_para` resolve to.
async fn fund_sender_account(
	dest_client: &OnlineClient<PolkadotConfig>,
	source_para: u32,
) -> anyhow::Result<()> {
	let sender_on_dest = sender_account_on_dest(source_para, dev::alice().public_key().0)?;
	let sender_on_dest = AccountId32(*sender_on_dest.as_ref());
	let fund_tx =
		penpal::tx()
			.sudo()
			.sudo(PenpalRuntimeCall::Balances(BalancesCall::force_set_balance {
				who: MultiAddress::Id(sender_on_dest),
				new_free: SENDER_FUNDS,
			}));
	dest_client
		.tx()
		.sign_and_submit_then_watch_default(&fund_tx, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(())
}

/// Free balance of `account`.
async fn free_balance(
	client: &OnlineClient<PolkadotConfig>,
	account: [u8; 32],
) -> anyhow::Result<u128> {
	Ok(client
		.storage()
		.at_latest()
		.await?
		.fetch(&penpal::storage().system().account(AccountId32(account)))
		.await?
		.map(|a| a.data.free)
		.unwrap_or_default())
}

/// Sets or clears the `HrmpClosing` cutover flag for `peer` via sudo.
async fn set_hrmp_closing(
	client: &OnlineClient<PolkadotConfig>,
	peer: u32,
	closing: bool,
) -> anyhow::Result<()> {
	let call = if closing {
		SpecMessagingCall::set_hrmp_closing { peer: PenpalId(peer) }
	} else {
		SpecMessagingCall::clear_hrmp_closing { peer: PenpalId(peer) }
	};
	let tx = penpal::tx().sudo().sudo(PenpalRuntimeCall::SpecMessaging(call));
	client
		.tx()
		.sign_and_submit_then_watch_default(&tx, &dev::alice())
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(())
}

/// Waits until the HRMP channel `sender -> recipient` exists on the relay
/// chain (`force_open_hrmp_channel` enacts at the next session boundary).
async fn wait_for_hrmp_channel(
	relay_client: &OnlineClient<PolkadotConfig>,
	sender: u32,
	recipient: u32,
	open: bool,
) -> anyhow::Result<()> {
	for _ in 0..100 {
		let channel_id = HrmpChannelId { sender: Id(sender), recipient: Id(recipient) };
		let channel = relay_client
			.storage()
			.at_latest()
			.await?
			.fetch(&rococo::storage().hrmp().hrmp_channels(channel_id))
			.await?;
		if channel.is_some() == open {
			return Ok(());
		}
		tokio::time::sleep(Duration::from_secs(6)).await;
	}
	Err(anyhow!(
		"HRMP channel {sender} -> {recipient} did not become {} in time",
		if open { "open" } else { "closed" }
	))
}

/// Polls until the sender's outbound frontier of `stream` reaches at least
/// `leaf_count` leaves — the send left the runtime onto the spec-msg wire.
async fn wait_for_frontier(
	client: &OnlineClient<PolkadotConfig>,
	stream: &StreamId,
	leaf_count: u64,
) -> anyhow::Result<MmrFrontier> {
	for _ in 0..50 {
		if let Some(frontier) = outbound_frontier(client, stream).await? {
			if frontier.leaf_count >= leaf_count {
				return Ok(frontier);
			}
		}
		tokio::time::sleep(Duration::from_secs(3)).await;
	}
	Err(anyhow!("outbound frontier of {stream:?} did not reach {leaf_count} leaves in time"))
}

/// Polls until the relay's `RecentProvides` ring of `sender` is non-empty.
async fn wait_for_recent_provides(
	relay_client: &OnlineClient<PolkadotConfig>,
	sender: u32,
) -> anyhow::Result<Vec<(sp_core::H256, u32)>> {
	for _ in 0..50 {
		let ring = recent_provides(relay_client, sender).await?;
		if !ring.is_empty() {
			return Ok(ring);
		}
		tokio::time::sleep(Duration::from_secs(6)).await;
	}
	Err(anyhow!("relay RecentProvides ring of {sender} stayed empty"))
}

/// Scans up to `depth` recent finalized blocks of `client` for the `SPMS`
/// consensus header digest and returns the digest's payload (the block's
/// `StreamsRoot`).
async fn find_spms_digest(
	client: &OnlineClient<PolkadotConfig>,
	depth: u32,
) -> anyhow::Result<Option<Vec<u8>>> {
	let mut hash = client.blocks().at_latest().await?.hash();
	for _ in 0..depth {
		let block = client.blocks().at(hash).await?;
		for log in &block.header().digest.logs {
			if let DigestItem::Consensus(SPMS_ENGINE_ID, payload) = log {
				return Ok(Some(payload.clone()));
			}
		}
		if block.number() == 0 {
			break;
		}
		hash = block.header().parent_hash;
	}
	Ok(None)
}

/// Tallies every finalized `MessageQueue.Processed` event on `client` from
/// genesis to the current tip: `(spec_msg_count, sibling_count)` for
/// messages originating from `para`.
async fn count_processed(
	client: &OnlineClient<PolkadotConfig>,
	para: u32,
) -> anyhow::Result<(u32, u32)> {
	let tip = client.blocks().at_latest().await?;
	let mut hash = tip.hash();
	let (mut spec_msg, mut sibling) = (0, 0);
	loop {
		let block = client.blocks().at(hash).await?;
		let events = block.events().await?;
		for event in events.find::<penpal::message_queue::events::Processed>() {
			let event = event?;
			match &event.origin {
				AggregateMessageOrigin::SpecMsg(id) if id.0 == para => spec_msg += 1,
				AggregateMessageOrigin::Sibling(id) if id.0 == para => sibling += 1,
				_ => {},
			}
		}
		if block.number() == 0 {
			break;
		}
		hash = block.header().parent_hash;
	}
	Ok((spec_msg, sibling))
}

#[tokio::test(flavor = "multi_thread")]
async fn spec_msg_penpal_xcm_delivery() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let penpal_a_addr = collator_multiaddr("penpal-a", PENPAL_A_P2P_PORT)?;
	let penpal_b_addr = collator_multiaddr("penpal-b", PENPAL_B_P2P_PORT)?;
	log::info!("penpal-a serves spec-msg at {penpal_a_addr}");
	log::info!("penpal-b serves spec-msg at {penpal_b_addr}");

	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
				.with_node(|node| node.with_name("charlie"))
				.with_node(|node| node.with_name("dave"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_A)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("penpal-rococo-2000")
				.with_collator(|n| {
					n.with_name("penpal-a").with_p2p_port(PENPAL_A_P2P_PORT).with_args(vec![
						("--spec-msg-source-peer", format!("{PARA_B}={penpal_b_addr}").as_str())
							.into(),
						// The final exactly-once tally reads every block's events.
						("--state-pruning", "archive").into(),
						("-lspec-msg=debug").into(),
					])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("penpal-rococo-2001")
				.with_collator(|n| {
					n.with_name("penpal-b").with_p2p_port(PENPAL_B_P2P_PORT).with_args(vec![
						("--spec-msg-source-peer", format!("{PARA_A}={penpal_a_addr}").as_str())
							.into(),
						// The final exactly-once tally reads every block's events.
						("--state-pruning", "archive").into(),
						("-lspec-msg=debug").into(),
					])
				})
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_client: OnlineClient<PolkadotConfig> =
		network.get_node("alice")?.wait_client().await?;
	let para_a_client: OnlineClient<PolkadotConfig> =
		network.get_node("penpal-a")?.wait_client().await?;
	let para_b_client: OnlineClient<PolkadotConfig> =
		network.get_node("penpal-b")?.wait_client().await?;

	log::info!("Waiting for both parachains to produce blocks");
	assert_para_throughput(
		&relay_client,
		20,
		HashMap::from([(RelayParaId::from(PARA_A), 2..40), (RelayParaId::from(PARA_B), 2..40)]),
		[],
	)
	.await?;

	let alice = dev::alice();

	// === 1. Baseline: HRMP open both directions, XCM delivers over HRMP. ===
	log::info!("Force-opening HRMP channels {PARA_A} <-> {PARA_B}");
	let force_open = |sender: u32, recipient: u32| {
		RococoRuntimeCall::Hrmp(HrmpCall::force_open_hrmp_channel {
			sender: Id(sender),
			recipient: Id(recipient),
			max_capacity: HRMP_MAX_CAPACITY,
			max_message_size: HRMP_MAX_MESSAGE_SIZE,
		})
	};
	let open_channels_tx =
		rococo::tx().sudo().sudo(RococoRuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![force_open(PARA_A, PARA_B), force_open(PARA_B, PARA_A)],
		}));
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&open_channels_tx, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	wait_for_hrmp_channel(&relay_client, PARA_A, PARA_B, true).await?;
	wait_for_hrmp_channel(&relay_client, PARA_B, PARA_A, true).await?;
	log::info!("HRMP channels open");

	fund_sender_account(&para_b_client, PARA_A).await?;
	fund_sender_account(&para_a_client, PARA_B).await?;

	const HRMP_RECEIVER_1: [u8; 32] = [0xA1; 32];
	let processed = tokio::spawn({
		let client = para_b_client.clone();
		async move { wait_for_message_processed(&client, PARA_A, false).await }
	});
	let hrmp_started = Instant::now();
	log::info!("Sending XCM A -> B over HRMP (baseline)");
	send_xcm_to_sibling(&para_a_client, PARA_B, transfer_program(HRMP_RECEIVER_1), true).await?;
	let success = tokio::time::timeout(Duration::from_secs(300), processed)
		.await
		.map_err(|_| anyhow!("timed out waiting for the HRMP baseline delivery"))???;
	if !success {
		return Err(anyhow!("HRMP baseline message failed on penpal B"));
	}
	let hrmp_latency = hrmp_started.elapsed();
	log::info!("HRMP baseline delivery took {hrmp_latency:?}");

	// === 2. Arm spec-msg channels both directions; the wire must not change. ===
	log::info!("Arming spec-msg channels (placeholder arming via set_storage)");
	arm_spec_msg_channel(&para_a_client, PARA_A, PARA_B).await?;
	arm_spec_msg_channel(&para_b_client, PARA_B, PARA_A).await?;

	const HRMP_RECEIVER_2: [u8; 32] = [0xA2; 32];
	let processed = tokio::spawn({
		let client = para_b_client.clone();
		async move { wait_for_message_processed(&client, PARA_A, false).await }
	});
	log::info!("Sending XCM A -> B with both transports armed; HRMP must win");
	send_xcm_to_sibling(&para_a_client, PARA_B, transfer_program(HRMP_RECEIVER_2), true).await?;
	let success = tokio::time::timeout(Duration::from_secs(300), processed)
		.await
		.map_err(|_| anyhow!("timed out waiting for the HRMP-wins delivery"))???;
	if !success {
		return Err(anyhow!("HRMP-wins message failed on penpal B"));
	}
	// Nothing touched the spec-msg wire: no outbound frontier exists.
	if outbound_frontier(&para_a_client, &xcm_channel_stream(PARA_B)).await?.is_some() {
		return Err(anyhow!("spec-msg outbound frontier advanced while HRMP was open"));
	}
	log::info!("HRMP still wins while the channel is open; spec-msg wire untouched");

	// === 3. Cutover: divert new traffic, drain, close relay-side. ===
	log::info!("Setting HrmpClosing on both penpals");
	set_hrmp_closing(&para_a_client, PARA_B, true).await?;
	set_hrmp_closing(&para_b_client, PARA_A, true).await?;

	log::info!("Verifying both HRMP pipes drain before the close");
	wait_for_hrmp_drain(&para_a_client, &relay_client, PARA_A, PARA_B).await?;
	wait_for_hrmp_drain(&para_b_client, &relay_client, PARA_B, PARA_A).await?;

	log::info!("Closing the HRMP channels relay-side (force_clean_hrmp)");
	let close_tx = rococo::tx().sudo().sudo(RococoRuntimeCall::Hrmp(HrmpCall::force_clean_hrmp {
		para: Id(PARA_A),
		num_inbound: 1,
		num_outbound: 1,
	}));
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&close_tx, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	wait_for_hrmp_channel(&relay_client, PARA_A, PARA_B, false).await?;
	wait_for_hrmp_channel(&relay_client, PARA_B, PARA_A, false).await?;
	log::info!("HRMP channels closed");

	// === 4. Deliver over spec-msg, both directions. ===
	const SPEC_RECEIVER_1: [u8; 32] = [0xB1; 32];
	let processed = tokio::spawn({
		let client = para_b_client.clone();
		async move { wait_for_message_processed(&client, PARA_A, true).await }
	});
	let spec_started = Instant::now();
	log::info!("Sending XCM A -> B over spec-msg");
	send_xcm_to_sibling(&para_a_client, PARA_B, transfer_program(SPEC_RECEIVER_1), false).await?;

	// The send left the sender runtime onto the stream...
	let frontier = wait_for_frontier(&para_a_client, &xcm_channel_stream(PARA_B), 1).await?;
	log::info!("penpal A's outbound frontier advanced to {} leaf/leaves", frontier.leaf_count);
	// ...the committing block carries the SPMS header digest...
	if find_spms_digest(&para_a_client, 30).await?.is_none() {
		return Err(anyhow!("no SPMS consensus digest in penpal A's recent headers"));
	}
	// ...and the relay's per-sender ring gained the included root.
	let ring = wait_for_recent_provides(&relay_client, PARA_A).await?;
	log::info!("relay RecentProvides[{PARA_A}] holds {} root(s)", ring.len());

	// The receiver executes it under the SpecMsg (Sibling-identical) origin.
	let success = tokio::time::timeout(Duration::from_secs(600), processed)
		.await
		.map_err(|_| anyhow!("timed out waiting for the spec-msg delivery A -> B"))???;
	if !success {
		return Err(anyhow!("spec-msg message A -> B failed on penpal B"));
	}
	let spec_latency = spec_started.elapsed();
	let receiver_balance = free_balance(&para_b_client, SPEC_RECEIVER_1).await?;
	if receiver_balance == 0 || receiver_balance >= TRANSFER_AMOUNT {
		return Err(anyhow!("receiver should hold the deposit minus fees, got {receiver_balance}"));
	}
	log::info!("spec-msg delivery A -> B took {spec_latency:?} (HRMP baseline: {hrmp_latency:?})");

	// Reverse direction: B -> A.
	const SPEC_RECEIVER_2: [u8; 32] = [0xB2; 32];
	let processed = tokio::spawn({
		let client = para_a_client.clone();
		async move { wait_for_message_processed(&client, PARA_B, true).await }
	});
	log::info!("Sending XCM B -> A over spec-msg");
	send_xcm_to_sibling(&para_b_client, PARA_A, transfer_program(SPEC_RECEIVER_2), false).await?;
	wait_for_frontier(&para_b_client, &xcm_channel_stream(PARA_A), 1).await?;
	wait_for_recent_provides(&relay_client, PARA_B).await?;
	let success = tokio::time::timeout(Duration::from_secs(600), processed)
		.await
		.map_err(|_| anyhow!("timed out waiting for the spec-msg delivery B -> A"))???;
	if !success {
		return Err(anyhow!("spec-msg message B -> A failed on penpal A"));
	}
	let receiver_balance = free_balance(&para_a_client, SPEC_RECEIVER_2).await?;
	if receiver_balance == 0 || receiver_balance >= TRANSFER_AMOUNT {
		return Err(anyhow!("receiver should hold the deposit minus fees, got {receiver_balance}"));
	}
	log::info!("spec-msg delivery B -> A verified");

	// A second A -> B message: ordered delivery on the same stream.
	const SPEC_RECEIVER_3: [u8; 32] = [0xB3; 32];
	let processed = tokio::spawn({
		let client = para_b_client.clone();
		async move { wait_for_message_processed(&client, PARA_A, true).await }
	});
	log::info!("Sending a second XCM A -> B over spec-msg");
	send_xcm_to_sibling(&para_a_client, PARA_B, transfer_program(SPEC_RECEIVER_3), false).await?;
	wait_for_frontier(&para_a_client, &xcm_channel_stream(PARA_B), 2).await?;
	let success = tokio::time::timeout(Duration::from_secs(600), processed)
		.await
		.map_err(|_| anyhow!("timed out waiting for the second spec-msg delivery"))???;
	if !success {
		return Err(anyhow!("second spec-msg message A -> B failed on penpal B"));
	}
	let receiver_balance = free_balance(&para_b_client, SPEC_RECEIVER_3).await?;
	if receiver_balance == 0 {
		return Err(anyhow!("second spec-msg deposit did not arrive"));
	}
	log::info!("Second spec-msg delivery A -> B verified");

	// === 5. Rollback: re-open HRMP A -> B; the router reverts. ===
	log::info!("Rolling back: re-opening HRMP {PARA_A} -> {PARA_B}");
	let reopen_tx = rococo::tx().sudo().sudo(force_open(PARA_A, PARA_B));
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&reopen_tx, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	wait_for_hrmp_channel(&relay_client, PARA_A, PARA_B, true).await?;
	set_hrmp_closing(&para_a_client, PARA_B, false).await?;

	const HRMP_RECEIVER_3: [u8; 32] = [0xA3; 32];
	let processed = tokio::spawn({
		let client = para_b_client.clone();
		async move { wait_for_message_processed(&client, PARA_A, false).await }
	});
	log::info!("Sending XCM A -> B after the rollback; HRMP must carry it");
	send_xcm_to_sibling(&para_a_client, PARA_B, transfer_program(HRMP_RECEIVER_3), true).await?;
	let success = tokio::time::timeout(Duration::from_secs(300), processed)
		.await
		.map_err(|_| anyhow!("timed out waiting for the post-rollback HRMP delivery"))???;
	if !success {
		return Err(anyhow!("post-rollback HRMP message failed on penpal B"));
	}
	// The frontier did not move: the rollback message stayed off the stream.
	let frontier = outbound_frontier(&para_a_client, &xcm_channel_stream(PARA_B))
		.await?
		.ok_or_else(|| anyhow!("outbound frontier disappeared"))?;
	if frontier.leaf_count != 2 {
		return Err(anyhow!(
			"outbound frontier moved to {} leaves after the rollback",
			frontier.leaf_count
		));
	}
	log::info!("Rollback verified: traffic is back on HRMP, spec-msg stream untouched");

	// === 6. A sibling with neither transport is unroutable. ===
	log::info!("Sending to sibling {PARA_NOWHERE} with no transport; the send must fail");
	let unroutable =
		send_xcm_to_sibling(&para_a_client, PARA_NOWHERE, transfer_program([0xC1; 32]), false)
			.await;
	if unroutable.is_ok() {
		return Err(anyhow!("send to a sibling with no transport unexpectedly succeeded"));
	}
	log::info!("Send to {PARA_NOWHERE} failed as expected: {}", unroutable.unwrap_err());

	// === Exactly-once tally over the whole run. ===
	// The spec-msg counts are exact: B executed exactly the 2 messages A
	// sent over the stream and A exactly the 1 from B — nothing lost,
	// nothing duplicated, nothing crossed over to the Sibling origin
	// (per-message uniqueness is additionally pinned by the distinct
	// receiver deposits above). The Sibling tallies also contain
	// pallet-xcm version-discovery chatter (SubscribeVersion /
	// QueryResponse ride HRMP too), so the HRMP side is lower-bounded by
	// the 3 test messages (baseline, HRMP-wins, rollback) instead.
	let (spec_on_b, sibling_on_b) = count_processed(&para_b_client, PARA_A).await?;
	let (spec_on_a, _sibling_on_a) = count_processed(&para_a_client, PARA_B).await?;
	if spec_on_b != 2 {
		return Err(anyhow!(
			"penpal B processed {spec_on_b} spec-msg messages from A, expected exactly 2"
		));
	}
	if sibling_on_b < 3 {
		return Err(anyhow!(
			"penpal B processed {sibling_on_b} HRMP messages from A, expected at least 3"
		));
	}
	if spec_on_a != 1 {
		return Err(anyhow!(
			"penpal A processed {spec_on_a} spec-msg messages from B, expected exactly 1"
		));
	}
	log::info!(
		"Exactly-once delivery verified across flip and rollback; \
		 spec-msg latency {spec_latency:?} vs HRMP baseline {hrmp_latency:?}"
	);

	Ok(())
}
