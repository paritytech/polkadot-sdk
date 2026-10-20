// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Asset Hub and People rotate to a list of `AUTHORITIES` authorities, and each chain's running
//! collator
//! authors the block that enacts it.
//!
//! Setup:
//! * `westend-local` with validators `alice` and `bob`, and HRMP channels between Asset Hub and
//!   People sized for the announcement of the whole set.
//! * `asset-hub-westend-local` and `people-westend-local` with one invulnerable collator each.
//! * The relay chain writes storage on both parachains with `sudo` and an XCM `Transact` of
//!   `System::set_storage`, which the parachains execute as root.
//!
//! Test flow:
//! 1. Both parachains get session keys for the generated accounts, which with the invulnerable make
//!    `AUTHORITIES` authorities.
//! 2. Asset Hub starts era 1 with the generated accounts: the relay chain writes the planned era
//!    and staking's kept copy of its validators, then hands Asset Hub a session report that
//!    activates era 1. Staking's era-start hook stores the set on Asset Hub and sends it to People
//!    over HRMP, and both chains perform their two forced rotations.
//! 3. On each parachain the block that enacts the new authority list holds the generated accounts
//!    plus the invulnerable, has a header within the backing limit, and is included and finalized
//!    by the relay chain. As small extra bonus, the collator's log gives the size of the PoV that
//!    carries it.

use super::common::{
	account, build_network_config, fetch, fetch_at, fetch_raw_at, init_logging, keypair, node_seed,
	open_hrmp_channels, wait_for_event, wait_for_finalized_block, wait_for_hrmp_channel,
	NetworkOptions, ASSET_HUB_ID, ASSET_HUB_INVULNERABLE, CALL_TIMEOUT, CLIENT_TIMEOUT_SECS,
	PARA_BLOCKS, PEOPLE_ID, PEOPLE_INVULNERABLE, POLL_INTERVAL,
};
use crate::utils::initialize_network;

use anyhow::anyhow;
use codec::{Decode, Encode};
use cumulus_zombienet_sdk_helpers::submit_extrinsic_and_wait_for_finalization_success_with_timeout;
use polkadot_primitives::async_backing::Constraints;
use sp_consensus_aura::{sr25519::AuthorityId, ConsensusLog, AURA_ENGINE_ID};
use sp_core::crypto::key_types;
use sp_runtime::{generic::Header, traits::BlakeTwo256};
use std::{collections::BTreeSet, time::Duration};
use zombienet_sdk::{
	subxt::{
		self,
		config::substrate::DigestItem,
		dynamic::Value,
		ext::{
			scale_encode::EncodeAsType,
			scale_value::{At, Composite},
		},
		utils::H256,
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	LocalFileSystem, Network,
};

/// Generated accounts in the validator set. The number mimics a Polkadot Asset Hub with 600
/// validators and 14 collators: with the invulnerable it gives 614 authorities, close to the
/// backing limit on head data.
const SCALE_ACCOUNTS: u32 = 613;

/// The generated accounts plus the invulnerable.
const AUTHORITIES: usize = SCALE_ACCOUNTS as usize + 1;

/// Largest HRMP message between the two chains. The message carrying the whole set fits.
const HRMP_MAX_MESSAGE_SIZE: u32 = 64 * 1024;

/// The relay chain's `hrmp_channel_max_total_size`, room for one message of the largest size.
const HRMP_CHANNEL_MAX_TOTAL_SIZE: u32 = 100 * 1024;

/// Alice's balance on the relay chain, enough for the length fees of the sudo calls that carry
/// storage writes.
const SUDO_BALANCE: u128 = 1_000_000 * 1_000_000_000_000;

/// From the last sudo call writing session keys to the keys being in the parachain's finalized
/// state.
const KEYS_WRITTEN_TIMEOUT: Duration = Duration::from_secs(2 * 60);

/// From the session report that starts era 1 to People's second rotation: Asset Hub stores the
/// set, sends it over HRMP, People stores it and rotates twice, and the relay chain finalizes the
/// rotation.
const PEOPLE_ROTATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// From the finalization of the session report that starts era 1 to Asset Hub's second rotation
/// being finalized.
const ASSET_HUB_ROTATION_TIMEOUT: Duration = Duration::from_secs(4 * 60);

/// From a parachain's second rotation to the relay chain finalizing its inclusion.
const RELAY_FINALITY_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// From the relay chain finality to the collator's log holding the PoV of the rotation block.
const POV_LOG_TIMEOUT: Duration = Duration::from_secs(60);

/// A parachain under test.
struct Chain<'a> {
	name: &'static str,
	id: u32,
	client: &'a OnlineClient<PolkadotConfig>,
	invulnerable: &'static str,
}

/// The block that enacts a new Aura authority list.
/// Raw storage keys and values for `System::set_storage`.
type StorageItems = Vec<(Vec<u8>, Vec<u8>)>;

struct RotationBlock {
	hash: H256,
	number: u32,
	header_size: usize,
	authorities: usize,
}

/// The whole test, below the 60-minute CI step so that the step ends with this error and the log
/// collection steps run.
const TEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[tokio::test(flavor = "multi_thread")]
async fn rotation_to_a_polkadot_sized_authority_list_on_asset_hub_and_people(
) -> Result<(), anyhow::Error> {
	init_logging();

	tokio::time::timeout(TEST_TIMEOUT, run())
		.await
		.map_err(|_| anyhow!("the scale test did not finish within {TEST_TIMEOUT:?}"))?
}

async fn run() -> Result<(), anyhow::Error> {
	log::info!("Spawning network");
	let config = build_network_config(NetworkOptions {
		validator_collators: false,
		election: false,
		hrmp_channel_max_total_size: Some(HRMP_CHANNEL_MAX_TOTAL_SIZE),
		sudo_balance: Some(SUDO_BALANCE),
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
	let asset_hub = Chain {
		name: "Asset Hub",
		id: ASSET_HUB_ID,
		client: &asset_hub_client,
		invulnerable: ASSET_HUB_INVULNERABLE,
	};
	let people = Chain {
		name: "People",
		id: PEOPLE_ID,
		client: &people_client,
		invulnerable: PEOPLE_INVULNERABLE,
	};

	open_hrmp_channels(&relay_client, HRMP_MAX_MESSAGE_SIZE).await?;
	log::info!("Waiting for both parachains to finalize {PARA_BLOCKS} blocks");
	tokio::try_join!(
		wait_for_finalized_block(&asset_hub_client, asset_hub.name, PARA_BLOCKS),
		wait_for_finalized_block(&people_client, people.name, PARA_BLOCKS),
	)?;
	wait_for_hrmp_channel(&relay_client, ASSET_HUB_ID, PEOPLE_ID).await?;

	// GIVEN the generated accounts with session keys on both parachains
	let accounts = (0..SCALE_ACCOUNTS)
		.map(|i| Ok(account(&keypair(&format!("//scale//{i}"))?)))
		.collect::<Result<Vec<_>, anyhow::Error>>()?;
	for chain in [&people, &asset_hub] {
		let items = session_key_items(chain.client, &accounts)?;
		let keys = write_storage(&relay_client, chain, items).await?;
		wait_for_keys(chain, &keys).await?;
	}

	// WHEN Asset Hub starts era 1 with the generated accounts
	let people_index = current_index(&people_client).await?;
	let asset_hub_index = current_index(&asset_hub_client).await?;
	let asset_hub_received = tokio::spawn(wait_for_event(
		asset_hub_client.clone(),
		asset_hub.name,
		"ValidatorCollators",
		"ValidatorSetReceived",
		is_set_of_era_1_at_scale,
		ASSET_HUB_ROTATION_TIMEOUT,
	));
	let people_received = tokio::spawn(wait_for_event(
		people_client.clone(),
		people.name,
		"ValidatorCollators",
		"ValidatorSetReceived",
		is_set_of_era_1_at_scale,
		PEOPLE_ROTATION_TIMEOUT,
	));
	// Subscribed before the report so the rotation cannot be missed, timed from the report below.
	let asset_hub_rotation = tokio::spawn(wait_for_authorities_change(
		asset_hub_client.clone(),
		asset_hub.name,
		TEST_TIMEOUT,
	));
	let people_rotation = tokio::spawn(wait_for_authorities_change(
		people_client.clone(),
		people.name,
		PEOPLE_ROTATION_TIMEOUT,
	));
	log_latest_pov(&network, &asset_hub).await?;
	log_latest_pov(&network, &people).await?;
	start_era_1(&relay_client, &asset_hub, &accounts).await?;
	let asset_hub_rotation = tokio::time::timeout(ASSET_HUB_ROTATION_TIMEOUT, asset_hub_rotation);
	asset_hub_received.await??;
	people_received.await??;

	// THEN both chains rotate to the generated accounts plus the invulnerable in a block the relay
	// chain includes and finalizes
	let rotation = people_rotation.await??;
	check_rotation(&relay_client, &network, &people, &accounts, &rotation, people_index).await?;
	let rotation = asset_hub_rotation.await.map_err(|_| {
		anyhow!("Asset Hub did not rotate within {ASSET_HUB_ROTATION_TIMEOUT:?} of the report")
	})???;
	check_rotation(&relay_client, &network, &asset_hub, &accounts, &rotation, asset_hub_index)
		.await?;

	log::info!("Test finished successfully");
	Ok(())
}

/// `ValidatorSetReceived { era: 1, count: SCALE_ACCOUNTS }`.
fn is_set_of_era_1_at_scale(fields: &Composite<u32>) -> bool {
	fields.at("era").and_then(|era| era.as_u128()) == Some(1) &&
		fields.at("count").and_then(|count| count.as_u128()) == Some(SCALE_ACCOUNTS.into())
}

/// A raw storage item of `pallet` and `item` at `keys`, with the key hashed and the value
/// encoded as the chain's metadata describes them.
fn storage_item(
	client: &OnlineClient<PolkadotConfig>,
	pallet: &str,
	item: &str,
	keys: Vec<Value>,
	value: Value,
) -> Result<(Vec<u8>, Vec<u8>), anyhow::Error> {
	let key = client.storage().address_bytes(&subxt::dynamic::storage(pallet, item, keys))?;
	let metadata = client.metadata();
	let value_ty = metadata
		.pallet_by_name(pallet)
		.and_then(|pallet| pallet.storage())
		.and_then(|storage| storage.entry_by_name(item))
		.ok_or_else(|| anyhow!("no storage item `{pallet}::{item}` in the metadata"))?
		.entry_type()
		.value_ty();
	let value = value.encode_as_type(value_ty, metadata.types())?;
	Ok((key, value))
}

/// `Session::NextKeys` and `Session::KeyOwner` for each account, whose own public key is its
/// Aura key. `is_registered` reads `NextKeys`, and `KeyOwner` keeps pallet-session consistent.
fn session_key_items(
	client: &OnlineClient<PolkadotConfig>,
	accounts: &[[u8; 32]],
) -> Result<StorageItems, anyhow::Error> {
	let aura = key_types::AURA.0;
	accounts
		.iter()
		.flat_map(|account| {
			let keys = Value::named_composite([(
				"aura",
				Value::unnamed_composite([Value::from_bytes(account)]),
			)]);
			let key_owner = Value::unnamed_composite([
				Value::unnamed_composite([Value::from_bytes(aura)]),
				Value::from_bytes(account),
			]);
			[
				storage_item(client, "Session", "NextKeys", vec![Value::from_bytes(account)], keys),
				storage_item(
					client,
					"Session",
					"KeyOwner",
					vec![key_owner],
					Value::from_bytes(account),
				),
			]
		})
		.collect()
}

/// Starts era 1 on Asset Hub with `accounts` as its validators, without an election.
///
/// The relay chain writes era 1 as the planned era and as staking's kept copy of its validators,
/// then sends `relay_session_report` with an activation of era 1, which ends era 0 and calls the
/// era-start hook.
async fn start_era_1(
	relay_client: &OnlineClient<PolkadotConfig>,
	asset_hub: &Chain<'_>,
	accounts: &[[u8; 32]],
) -> Result<(), anyhow::Error> {
	let validators = Value::unnamed_composite(accounts.iter().map(Value::from_bytes));
	let items = vec![
		storage_item(asset_hub.client, "Staking", "CurrentEra", vec![], Value::u128(1))?,
		storage_item(
			asset_hub.client,
			"Staking",
			"NextEraValidators",
			vec![],
			Value::unnamed_composite([Value::u128(1), validators]),
		)?,
	];
	write_storage(relay_client, asset_hub, items).await?;

	// Past any report the relay chain may have sent, so the report is not taken as a repeat.
	let last_report: Option<u32> =
		fetch(asset_hub.client, "StakingRcClient", "LastSessionReportEndingIndex", vec![]).await?;
	let end_index = last_report.map_or(0, |last| last + 100);
	let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
	let report = Value::named_composite([
		("end_index", Value::u128(end_index.into())),
		("validator_points", Value::unnamed_composite([])),
		(
			"activation_timestamp",
			Value::unnamed_variant(
				"Some",
				[Value::unnamed_composite([Value::u128(now), Value::u128(1)])],
			),
		),
		("leftover", Value::bool(false)),
	]);
	let call = asset_hub.client.tx().call_data(&subxt::dynamic::tx(
		"StakingRcClient",
		"relay_session_report",
		vec![report],
	))?;
	submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		relay_client,
		&root_transact(asset_hub.id, call),
		&dev::alice(),
		CALL_TIMEOUT.as_secs(),
	)
	.await
	.map_err(|e| anyhow!("sending the session report to {}: {e}", asset_hub.name))?;
	log::info!("Sent the session report that starts era 1 to {}", asset_hub.name);
	Ok(())
}

/// Writes `items` on `chain` from the relay chain, in as many messages as the chain's message
/// queue takes, and returns the last key of each message.
async fn write_storage(
	relay_client: &OnlineClient<PolkadotConfig>,
	chain: &Chain<'_>,
	items: StorageItems,
) -> Result<Vec<Vec<u8>>, anyhow::Error> {
	let heap_size: u32 = chain
		.client
		.constants()
		.at(&subxt::dynamic::constant("MessageQueue", "HeapSize"))?
		.as_type()?;
	// Half the page leaves room for the XCM around the call.
	let budget = heap_size as usize / 2;
	let mut chunks: Vec<StorageItems> = vec![vec![]];
	let mut size = 0;
	for item in items {
		let item_size = item.encoded_size();
		if size + item_size > budget {
			chunks.push(vec![]);
			size = 0;
		}
		size += item_size;
		chunks.last_mut().expect("`chunks` starts with one entry; qed").push(item);
	}

	let mut last_keys = vec![];
	for (index, chunk) in chunks.iter().enumerate() {
		let items = chunk
			.iter()
			.map(|(key, value)| {
				Value::unnamed_composite([Value::from_bytes(key), Value::from_bytes(value)])
			})
			.collect::<Vec<_>>();
		let call = chain.client.tx().call_data(&subxt::dynamic::tx(
			"System",
			"set_storage",
			vec![Value::unnamed_composite(items)],
		))?;
		submit_extrinsic_and_wait_for_finalization_success_with_timeout(
			relay_client,
			&root_transact(chain.id, call.clone()),
			&dev::alice(),
			CALL_TIMEOUT.as_secs(),
		)
		.await
		.map_err(|e| anyhow!("writing storage on {}: {e}", chain.name))?;
		log::info!(
			"Sent {} storage items to {} ({} of {}, call of {} bytes)",
			chunk.len(),
			chain.name,
			index + 1,
			chunks.len(),
			call.len()
		);
		last_keys.extend(chunk.last().map(|(key, _)| key.clone()));
	}
	Ok(last_keys)
}

/// `Sudo::sudo(XcmPallet::send)` of an unpaid `Transact` of `call` to parachain `para_id`, which
/// the parachain dispatches as root.
fn root_transact(para_id: u32, call: Vec<u8>) -> subxt::tx::DynamicPayload {
	let dest = Value::unnamed_variant(
		"V5",
		[Value::named_composite([
			("parents", Value::u128(0)),
			(
				"interior",
				Value::unnamed_variant(
					"X1",
					[Value::unnamed_composite([Value::unnamed_variant(
						"Parachain",
						[Value::u128(para_id.into())],
					)])],
				),
			),
		])],
	);
	let message = Value::unnamed_variant(
		"V5",
		[Value::unnamed_composite([Value::unnamed_composite([
			Value::named_variant(
				"UnpaidExecution",
				[
					("weight_limit", Value::unnamed_variant("Unlimited", [])),
					("check_origin", Value::unnamed_variant("None", [])),
				],
			),
			Value::named_variant(
				"Transact",
				[
					("origin_kind", Value::unnamed_variant("Superuser", [])),
					("fallback_max_weight", Value::unnamed_variant("None", [])),
					("call", Value::named_composite([("encoded", Value::from_bytes(call))])),
				],
			),
		])])],
	);
	subxt::dynamic::tx(
		"Sudo",
		"sudo",
		vec![Value::unnamed_variant(
			"XcmPallet",
			[Value::named_variant("send", [("dest", dest), ("message", message)])],
		)],
	)
}

/// Waits until the finalized state of `chain` holds every key in `keys`.
async fn wait_for_keys(chain: &Chain<'_>, keys: &[Vec<u8>]) -> Result<(), anyhow::Error> {
	let wait = async {
		loop {
			let storage = chain.client.storage().at_latest().await?;
			let mut missing = 0;
			for key in keys {
				if storage.fetch_raw(key.clone()).await?.is_none() {
					missing += 1;
				}
			}
			if missing == 0 {
				log::info!("{} holds the session keys of {SCALE_ACCOUNTS} accounts", chain.name);
				return Ok::<_, anyhow::Error>(());
			}
			tokio::time::sleep(POLL_INTERVAL).await;
		}
	};
	tokio::time::timeout(KEYS_WRITTEN_TIMEOUT, wait).await.map_err(|_| {
		anyhow!("{} did not store the session keys within {KEYS_WRITTEN_TIMEOUT:?}", chain.name)
	})?
}

async fn current_index(client: &OnlineClient<PolkadotConfig>) -> Result<u32, anyhow::Error> {
	Ok(fetch(client, "Session", "CurrentIndex", vec![]).await?.unwrap_or_default())
}

/// Waits for a finalized block whose header carries Aura's `AuthoritiesChange` digest. Only
/// finalized blocks count, since the collator may build blocks that do not become canonical.
async fn wait_for_authorities_change(
	client: OnlineClient<PolkadotConfig>,
	chain: &'static str,
	timeout: Duration,
) -> Result<RotationBlock, anyhow::Error> {
	let wait = async {
		let mut blocks = client.blocks().subscribe_finalized().await?;
		while let Some(block) = blocks.next().await {
			let block = block?;
			let header = block.header();
			let authorities = header.digest.logs.iter().find_map(|log| match log {
				DigestItem::Consensus(engine, data) if *engine == AURA_ENGINE_ID => {
					match ConsensusLog::<AuthorityId>::decode(&mut &data[..]) {
						Ok(ConsensusLog::AuthoritiesChange(authorities)) => Some(authorities.len()),
						_ => None,
					}
				},
				_ => None,
			});
			if let Some(authorities) = authorities {
				let rotation = RotationBlock {
					hash: block.hash(),
					number: block.number(),
					header_size: header.encoded_size(),
					authorities,
				};
				log::info!(
					"{chain} #{} {:?} enacts {authorities} Aura authorities, header of {} bytes",
					rotation.number,
					rotation.hash,
					rotation.header_size
				);
				return Ok(rotation);
			}
		}
		Err(anyhow!("{chain} block subscription ended"))
	};
	tokio::time::timeout(timeout, wait)
		.await
		.map_err(|_| anyhow!("no Aura authorities change on {chain} within {timeout:?}"))?
}

/// Checks the rotation block of `chain`: its authorities and session, its header size, its
/// inclusion and finality on the relay chain, and the size of the PoV carrying it.
async fn check_rotation(
	relay_client: &OnlineClient<PolkadotConfig>,
	network: &Network<LocalFileSystem>,
	chain: &Chain<'_>,
	accounts: &[[u8; 32]],
	rotation: &RotationBlock,
	index_before: u32,
) -> Result<(), anyhow::Error> {
	let expected = accounts
		.iter()
		.copied()
		.chain([account(&keypair(&node_seed(chain.invulnerable))?)])
		.collect::<BTreeSet<_>>();

	let authorities: Vec<[u8; 32]> =
		fetch_at(chain.client, rotation.hash, "Aura", "Authorities", vec![])
			.await?
			.unwrap_or_default();
	let validators: Vec<[u8; 32]> =
		fetch_at(chain.client, rotation.hash, "Session", "Validators", vec![])
			.await?
			.unwrap_or_default();
	let index: u32 = fetch_at(chain.client, rotation.hash, "Session", "CurrentIndex", vec![])
		.await?
		.unwrap_or_default();
	log::info!(
		"{} #{}: {} Aura authorities, {} session validators, session {index_before} to {index}",
		chain.name,
		rotation.number,
		authorities.len(),
		validators.len()
	);
	// Every generated account is its own Aura key, and zombienet derives the invulnerable's Aura
	// key and account from the same seed.
	if rotation.authorities != AUTHORITIES {
		return Err(anyhow!(
			"{} enacted {} authorities, expected {AUTHORITIES}",
			chain.name,
			rotation.authorities
		));
	}
	if authorities.into_iter().collect::<BTreeSet<_>>() != expected {
		return Err(anyhow!(
			"{} Aura authorities are not the invulnerable and the set",
			chain.name
		));
	}
	if validators.into_iter().collect::<BTreeSet<_>>() != expected {
		return Err(anyhow!(
			"{} session validators are not the invulnerable and the set",
			chain.name
		));
	}
	if index != index_before + 2 {
		return Err(anyhow!("{} session index went from {index_before} to {index}", chain.name));
	}

	let max_head_data_size = Constraints::<u32>::DEFAULT_MAX_HEAD_DATA_SIZE as usize;
	log::info!(
		"{} rotation header: {} bytes, backing limit {max_head_data_size}",
		chain.name,
		rotation.header_size
	);
	if rotation.header_size > max_head_data_size {
		return Err(anyhow!(
			"{} rotation header of {} bytes exceeds {max_head_data_size}",
			chain.name,
			rotation.header_size
		));
	}

	wait_for_relay_finality(relay_client, chain, rotation).await?;

	let max_pov_size = relay_max_pov_size(relay_client).await?;
	let compressed_pov_size = pov_size(network, chain, rotation).await?;
	if compressed_pov_size >= max_pov_size as f64 {
		return Err(anyhow!(
			"{} PoV of {compressed_pov_size} bytes reaches the relay's {max_pov_size}",
			chain.name
		));
	}
	Ok(())
}

/// Waits until `Paras::Heads(chain)` in a finalized relay chain block is the rotation block or a
/// later one. The head data is the encoded parachain header, so its hash is the block hash. A later
/// head passes unchecked: the rotation block comes from the parachain's finalized blocks, so any
/// later finalized head descends from it.
async fn wait_for_relay_finality(
	relay_client: &OnlineClient<PolkadotConfig>,
	chain: &Chain<'_>,
	rotation: &RotationBlock,
) -> Result<(), anyhow::Error> {
	let wait = async {
		let mut blocks = relay_client.blocks().subscribe_finalized().await?;
		while let Some(block) = blocks.next().await {
			let block = block?;
			let Some(head) = fetch_raw_at(
				relay_client,
				block.hash(),
				"Paras",
				"Heads",
				vec![Value::u128(chain.id.into())],
			)
			.await?
			else {
				continue;
			};
			let head = Vec::<u8>::decode(&mut &head[..])?;
			let number = Header::<u32, BlakeTwo256>::decode(&mut &head[..])?.number;
			let is_rotation = sp_crypto_hashing::blake2_256(&head) == rotation.hash.0;
			if number == rotation.number && !is_rotation {
				return Err(anyhow!("relay chain holds another {} #{number}", chain.name));
			}
			if number >= rotation.number {
				log::info!(
					"Relay chain #{} finalized with {} head #{number}{}",
					block.number(),
					chain.name,
					if is_rotation { ", the rotation block" } else { "" }
				);
				return Ok(());
			}
		}
		Err(anyhow!("relay chain block subscription ended"))
	};
	tokio::time::timeout(RELAY_FINALITY_TIMEOUT, wait).await.map_err(|_| {
		anyhow!(
			"relay chain did not finalize {} #{} within {RELAY_FINALITY_TIMEOUT:?}",
			chain.name,
			rotation.number
		)
	})?
}

async fn relay_max_pov_size(
	relay_client: &OnlineClient<PolkadotConfig>,
) -> Result<u32, anyhow::Error> {
	let config = relay_client
		.storage()
		.at_latest()
		.await?
		.fetch(&subxt::dynamic::storage("Configuration", "ActiveConfig", vec![]))
		.await?
		.ok_or_else(|| anyhow!("relay chain `Configuration::ActiveConfig` is not set"))?
		.to_value()?;
	config
		.at("max_pov_size")
		.and_then(|size| size.as_u128())
		.and_then(|size| u32::try_from(size).ok())
		.ok_or_else(|| anyhow!("relay chain configuration has no `max_pov_size`"))
}

/// A collation as the collator logs it: `Pre-sealed block for proposal at … Hash now 0x…` for each
/// block, then `PoV size header_kb=… extrinsics_kb=… storage_proof_kb=…` and `Compressed PoV size:
/// …kb block_numbers=[…]`.
struct Collation {
	/// Hashes of the blocks sealed since the previous collation.
	sealed: Vec<String>,
	blocks: Vec<u32>,
	sizes: String,
	compressed_kb: f64,
}

fn collations(logs: &str) -> Result<Vec<Collation>, anyhow::Error> {
	const SEALED: &str = "Hash now ";
	const SIZES: &str = "header_kb=";
	const COMPRESSED: &str = "Compressed PoV size: ";
	const BLOCKS: &str = "block_numbers=[";

	let mut collations = vec![];
	let mut sealed = vec![];
	let mut sizes = String::new();
	for line in logs.lines() {
		if let Some(start) = line.find(SEALED) {
			let hash = &line[start + SEALED.len()..];
			sealed.push(hash.split(',').next().unwrap_or(hash).trim().to_string());
		} else if let Some(start) = line.find(SIZES) {
			sizes = line[start..].trim().to_string();
		} else if let Some(start) = line.find(COMPRESSED) {
			let rest = &line[start + COMPRESSED.len()..];
			let (compressed_kb, rest) = rest
				.split_once("kb")
				.ok_or_else(|| anyhow!("unexpected PoV log line: {line}"))?;
			let blocks = rest
				.split_once(BLOCKS)
				.and_then(|(_, blocks)| blocks.split_once(']'))
				.map(|(blocks, _)| blocks)
				.ok_or_else(|| anyhow!("PoV log line without block numbers: {line}"))?;
			collations.push(Collation {
				sealed: std::mem::take(&mut sealed),
				blocks: blocks.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>()?,
				sizes: std::mem::take(&mut sizes),
				compressed_kb: compressed_kb.trim().parse()?,
			});
		}
	}
	Ok(collations)
}

/// Logs the collator's latest collation as the baseline for the rotation's PoV.
async fn log_latest_pov(
	network: &Network<LocalFileSystem>,
	chain: &Chain<'_>,
) -> Result<(), anyhow::Error> {
	let logs = tokio::time::timeout(CALL_TIMEOUT, network.get_node(chain.invulnerable)?.logs())
		.await
		.map_err(|_| {
			anyhow!("reading the log of `{}` took over {CALL_TIMEOUT:?}", chain.invulnerable)
		})??;
	let latest = collations(&logs)?
		.pop()
		.ok_or_else(|| anyhow!("`{}` logged no collation", chain.invulnerable))?;
	log::info!(
		"{} PoV before the rotation, blocks {:?}: {}, compressed {} kb",
		chain.name,
		latest.blocks,
		latest.sizes,
		latest.compressed_kb
	);
	Ok(())
}

/// Reads the collator's log for the collation carrying the rotation block and returns its
/// compressed PoV size in bytes. The collator can build a block number more than once, so the
/// collation is the one that sealed the rotation block's hash.
async fn pov_size(
	network: &Network<LocalFileSystem>,
	chain: &Chain<'_>,
	rotation: &RotationBlock,
) -> Result<f64, anyhow::Error> {
	let hash = format!("{:?}", rotation.hash);
	let node = network.get_node(chain.invulnerable)?;
	let wait = async {
		loop {
			let logs = node.logs().await?;
			let found = collations(&logs)?.into_iter().find(|collation| {
				collation.sealed.contains(&hash) && collation.blocks.contains(&rotation.number)
			});
			if let Some(collation) = found {
				log::info!(
					"{} PoV of the rotation, blocks {:?}: {}, compressed {} kb",
					chain.name,
					collation.blocks,
					collation.sizes,
					collation.compressed_kb
				);
				return Ok::<_, anyhow::Error>(collation.compressed_kb * 1024.0);
			}
			tokio::time::sleep(POLL_INTERVAL).await;
		}
	};
	tokio::time::timeout(POV_LOG_TIMEOUT, wait).await.map_err(|_| {
		anyhow!(
			"`{}` logged no PoV for #{} within {POV_LOG_TIMEOUT:?}",
			chain.invulnerable,
			rotation.number
		)
	})?
}
