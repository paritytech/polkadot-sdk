// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Parachain-to-parachain XCM delivery over HRMP.
//!
//! Spawns a rococo-local relay chain and two penpal parachains, then opens HRMP
//! channels in both directions with `force_open_hrmp_channel` via relay sudo (the
//! same flow the bridges zombienet environments use — zombienet's genesis-level
//! `hrmp_channels` config only patches genesis keys that are already present, and
//! preset-based runtime genesis omits the `hrmp` section entirely).
//!
//! Alice on penpal A sends an XCM program to sibling penpal B which withdraws from
//! her derived account on B, buys execution with penpal B's native token and
//! deposits the remainder into a fresh receiver account.
//!
//! Delivery is asserted on B via `MessageQueue.Processed { origin: Sibling(A) }`
//! plus a non-zero receiver balance.
//!
//! The test is transport-focused on purpose: to move it to another para-to-para
//! transport (e.g. speculative messaging), swap the channel setup and keep the
//! send/assert harness.

use anyhow::anyhow;
use std::{collections::HashMap, time::Duration};

use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use sp_core::crypto::AccountId32 as SpAccountId32;
use xcm::latest::{
	Junction as XcmJunction, Location as XcmLocation, NetworkId, WESTEND_GENESIS_HASH,
};
use xcm_builder::{DescribeAllTerminal, DescribeFamily, DescribeTerminus, HashedDescription};
use xcm_executor::traits::ConvertLocation;
use zombienet_sdk::{
	subxt::{
		utils::{AccountId32, MultiAddress},
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

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
	cumulus_primitives_core::AggregateMessageOrigin,
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

// HRMP channel parameters, well within rococo-local's host configuration limits.
const HRMP_MAX_CAPACITY: u32 = 4;
const HRMP_MAX_MESSAGE_SIZE: u32 = 524288;

// Penpal uses 12 decimals ("UNIT").
const UNIT: u128 = 1_000_000_000_000;
// Balance given to the sender's derived account on B, in B's native token.
const SENDER_FUNDS_ON_B: u128 = 100 * UNIT;
// Amount withdrawn by the XCM program on B; covers fees, the rest is deposited.
const TRANSFER_AMOUNT: u128 = 10 * UNIT;
// Fresh, deterministic receiver account, not endowed at genesis.
const RECEIVER: [u8; 32] = [7u8; 32];

/// The account on penpal B that the incoming XCM's origin resolves to.
///
/// `pallet_xcm::send` on A prepends `DescendOrigin(AccountId32 { alice })`, so B sees the
/// origin `(1, [Parachain(PARA_A), AccountId32 { alice }])` and converts it to an account
/// via `HashedDescription` — the same converter penpal's `LocationToAccountId` ends up in.
/// The network id is what A's `SignedToAccountId32` stamps: penpal's `RelayNetworkId`
/// storage default, `ByGenesis(WESTEND_GENESIS_HASH)`, regardless of the actual relay.
fn sender_account_on_b(sender: [u8; 32]) -> anyhow::Result<SpAccountId32> {
	let origin_on_b = XcmLocation::new(
		1,
		[
			XcmJunction::Parachain(PARA_A),
			XcmJunction::AccountId32 {
				network: Some(NetworkId::ByGenesis(WESTEND_GENESIS_HASH)),
				id: sender,
			},
		],
	);
	HashedDescription::<SpAccountId32, (DescribeTerminus, DescribeFamily<DescribeAllTerminal>)>::convert_location(
		&origin_on_b,
	)
	.ok_or_else(|| anyhow!("failed to derive the sender's account on penpal B"))
}

/// Waits until the HRMP channel `sender -> recipient` exists on the relay chain.
///
/// `force_open_hrmp_channel` takes effect at the next session boundary, so this can
/// take up to a full session.
async fn wait_for_hrmp_channel(
	relay_client: &OnlineClient<PolkadotConfig>,
	sender: u32,
	recipient: u32,
) -> anyhow::Result<()> {
	for _ in 0..100 {
		let channel_id = HrmpChannelId { sender: Id(sender), recipient: Id(recipient) };
		let channel = relay_client
			.storage()
			.at_latest()
			.await?
			.fetch(&rococo::storage().hrmp().hrmp_channels(channel_id))
			.await?;
		if channel.is_some() {
			return Ok(());
		}
		tokio::time::sleep(Duration::from_secs(6)).await;
	}
	Err(anyhow!("HRMP channel {sender} -> {recipient} did not open in time"))
}

/// Waits until the HRMP pipe `sender -> recipient` is fully drained — the scripted
/// drain verification of the HRMP→spec-msg cutover runbook (drain-before-close):
/// a closure enacting on a non-empty pipe LOSES messages (the sender's
/// `take_outbound_messages` swallows every page buffered for a `Closed` destination;
/// the relay's `close_hrmp_channel` drops all undelivered contents), so this must
/// report success before `hrmp.close_channel`'s session boundary.
///
/// Drained means, at once:
/// - the sender's `xcmpQueue.outboundXcmpStatus` entry for `recipient` (if any) holds no queued
///   pages (`first_index == last_index`) and no pending signals, and no
///   `xcmpQueue.outboundXcmpMessages` page for `recipient` remains, and
/// - the relay's `hrmp.hrmpChannelContents` of the pair is empty — the recipient's watermark caught
///   up and `prune_hrmp` ran.
///
/// Polls, since draining takes as long as the recipient needs to consume the backlog.
pub(crate) async fn wait_for_hrmp_drain(
	sender_client: &OnlineClient<PolkadotConfig>,
	relay_client: &OnlineClient<PolkadotConfig>,
	sender: u32,
	recipient: u32,
) -> anyhow::Result<()> {
	for _ in 0..50 {
		if hrmp_drained(sender_client, relay_client, sender, recipient).await? {
			return Ok(());
		}
		tokio::time::sleep(Duration::from_secs(6)).await;
	}
	Err(anyhow!("HRMP pipe {sender} -> {recipient} did not drain in time"))
}

/// One-shot drain check underneath [`wait_for_hrmp_drain`].
async fn hrmp_drained(
	sender_client: &OnlineClient<PolkadotConfig>,
	relay_client: &OnlineClient<PolkadotConfig>,
	sender: u32,
	recipient: u32,
) -> anyhow::Result<bool> {
	// Sender side: no buffered pages or signals for the recipient.
	let sender_storage = sender_client.storage().at_latest().await?;
	let statuses = sender_storage
		.fetch(&penpal::storage().xcmp_queue().outbound_xcmp_status())
		.await?
		.map(|statuses| statuses.0)
		.unwrap_or_default();
	if statuses.iter().any(|channel| {
		channel.recipient.0 == recipient &&
			(channel.first_index != channel.last_index || channel.signals_exist)
	}) {
		return Ok(false);
	}
	let mut pages = sender_storage
		.iter(penpal::storage().xcmp_queue().outbound_xcmp_messages_iter1(PenpalId(recipient)))
		.await?;
	if pages.next().await.is_some() {
		return Ok(false);
	}

	// Relay side: no undelivered channel contents.
	let contents =
		relay_client
			.storage()
			.at_latest()
			.await?
			.fetch(&rococo::storage().hrmp().hrmp_channel_contents(HrmpChannelId {
				sender: Id(sender),
				recipient: Id(recipient),
			}))
			.await?
			.unwrap_or_default();
	Ok(contents.is_empty())
}

/// Waits (on finalized blocks) for a `MessageQueue.Processed` event for a message coming
/// from sibling `para`, and returns its `success` flag.
async fn wait_for_sibling_message_processed(
	client: &OnlineClient<PolkadotConfig>,
	para: u32,
) -> anyhow::Result<bool> {
	let mut blocks = client.blocks().subscribe_finalized().await?;
	while let Some(block) = blocks.next().await {
		let events = block?.events().await?;
		for event in events.find::<penpal::message_queue::events::Processed>() {
			let event = event?;
			if matches!(
				&event.origin,
				AggregateMessageOrigin::Sibling(id) if id.0 == para
			) {
				return Ok(event.success);
			}
		}
	}
	Err(anyhow!("block subscription ended before the message was processed"))
}

#[tokio::test(flavor = "multi_thread")]
async fn hrmp_penpal_xcm_delivery() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

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
				.with_collator(|n| n.with_name("penpal-a"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_B)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("penpal-rococo-2001")
				.with_collator(|n| n.with_name("penpal-b"))
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

	// Both parachains produce blocks; HRMP channels were opened at genesis.
	log::info!("Waiting for both parachains to produce blocks");
	assert_para_throughput(
		&relay_client,
		20,
		HashMap::from([(ParaId::from(PARA_A), 2..40), (ParaId::from(PARA_B), 2..40)]),
		[],
	)
	.await?;

	let alice = dev::alice();

	// Open HRMP channels in both directions via relay sudo, then wait for them to
	// come into effect (channels open at the session boundary following the call).
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
	wait_for_hrmp_channel(&relay_client, PARA_A, PARA_B).await?;
	wait_for_hrmp_channel(&relay_client, PARA_B, PARA_A).await?;
	log::info!("HRMP channels open");

	// Fund, on B, the account the incoming message's origin resolves to. Alice is
	// penpal's sudo key.
	let sender_on_b = sender_account_on_b(alice.public_key().0)?;
	let sender_on_b = AccountId32(*sender_on_b.as_ref());
	log::info!("Funding the sender's derived account on penpal B: {sender_on_b}");

	let fund_tx =
		penpal::tx()
			.sudo()
			.sudo(PenpalRuntimeCall::Balances(BalancesCall::force_set_balance {
				who: MultiAddress::Id(sender_on_b.clone()),
				new_free: SENDER_FUNDS_ON_B,
			}));
	para_b_client
		.tx()
		.sign_and_submit_then_watch_default(&fund_tx, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	let funded = para_b_client
		.storage()
		.at_latest()
		.await?
		.fetch(&penpal::storage().system().account(sender_on_b.clone()))
		.await?
		.map(|a| a.data.free)
		.unwrap_or_default();
	assert_eq!(funded, SENDER_FUNDS_ON_B, "sender's derived account on B is not funded");

	// Subscribe on B before sending on A so the Processed event cannot be missed.
	let processed = tokio::spawn({
		let para_b_client = para_b_client.clone();
		async move { wait_for_sibling_message_processed(&para_b_client, PARA_A).await }
	});

	// Withdraw B's native token from the sender's derived account, pay for execution
	// with it (penpal's trader accepts the native currency) and deposit the rest.
	let native_on_b = || Location { parents: 0, interior: Junctions::Here };
	let dest = VersionedLocation::V5(Location {
		parents: 1,
		interior: Junctions::X1([Junction::Parachain(PARA_B)]),
	});
	let message = VersionedXcm::V5(Xcm(vec![
		Instruction::WithdrawAsset(Assets(vec![Asset {
			id: AssetId(native_on_b()),
			fun: Fungibility::Fungible(TRANSFER_AMOUNT),
		}])),
		Instruction::BuyExecution {
			fees: Asset { id: AssetId(native_on_b()), fun: Fungibility::Fungible(TRANSFER_AMOUNT) },
			weight_limit: WeightLimit::Unlimited,
		},
		Instruction::DepositAsset {
			assets: AssetFilter::Wild(WildAsset::All),
			beneficiary: Location {
				parents: 0,
				interior: Junctions::X1([Junction::AccountId32 { network: None, id: RECEIVER }]),
			},
		},
	]));

	log::info!("Sending XCM from penpal A to penpal B over HRMP");
	let send_tx = penpal::tx().polkadot_xcm().send(dest, message);
	let send_events = para_a_client
		.tx()
		.sign_and_submit_then_watch_default(&send_tx, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	assert!(
		send_events.has::<penpal::polkadot_xcm::events::Sent>()?,
		"no PolkadotXcm.Sent event on penpal A"
	);
	// XcmpQueue is the HRMP transport: this proves the sibling router took the
	// message, not some other delivery path.
	assert!(
		send_events.has::<penpal::xcmp_queue::events::XcmpMessageSent>()?,
		"no XcmpQueue.XcmpMessageSent event on penpal A — message did not go out over HRMP"
	);
	log::info!("Message sent from penpal A via XcmpQueue (HRMP)");

	// The message must be processed successfully on B...
	let success =
		tokio::time::timeout(Duration::from_secs(300), processed).await.map_err(|_| {
			anyhow!("timed out waiting for the message to be processed on penpal B")
		})???;
	assert!(success, "message from penpal A was processed on penpal B but failed");
	log::info!("Message processed on penpal B");

	// ...the relay-side HRMP channel must have carried it: the message queue chain
	// head is a running hash that permanently moves off `None` once a message
	// traverses this specific channel.
	let channel = relay_client
		.storage()
		.at_latest()
		.await?
		.fetch(
			&rococo::storage()
				.hrmp()
				.hrmp_channels(HrmpChannelId { sender: Id(PARA_A), recipient: Id(PARA_B) }),
		)
		.await?
		.ok_or_else(|| anyhow!("HRMP channel {PARA_A} -> {PARA_B} disappeared"))?;
	assert!(
		channel.mqc_head.is_some(),
		"relay HRMP channel {PARA_A} -> {PARA_B} has an empty MQC head — no message ever traversed it"
	);
	log::info!("Relay HRMP channel MQC head: {:?}", channel.mqc_head);

	// ...and the deposit must have reached the receiver.
	let receiver_balance = para_b_client
		.storage()
		.at_latest()
		.await?
		.fetch(&penpal::storage().system().account(AccountId32(RECEIVER)))
		.await?
		.map(|a| a.data.free)
		.unwrap_or_default();
	assert!(
		receiver_balance > 0 && receiver_balance < TRANSFER_AMOUNT,
		"receiver should hold the deposit minus fees, got {receiver_balance}"
	);
	log::info!("Receiver got {receiver_balance} on penpal B; HRMP delivery verified");

	// Finally, exercise the cutover runbook's drain verification: with the message
	// delivered and processed, the A→B pipe must drain (B's watermark catches up, the
	// relay's `prune_hrmp` clears the contents) — the exact pre-close condition of the
	// HRMP→spec-msg cutover. The reverse direction, having carried nothing, reports
	// drained trivially.
	log::info!("Verifying both HRMP pipes report drained");
	wait_for_hrmp_drain(&para_a_client, &relay_client, PARA_A, PARA_B).await?;
	wait_for_hrmp_drain(&para_b_client, &relay_client, PARA_B, PARA_A).await?;
	log::info!("HRMP pipes drained; the pair would be safe to close");

	Ok(())
}
