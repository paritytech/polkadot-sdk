// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Network, storage and event helpers shared by the validator collators tests.

use anyhow::anyhow;
use codec::{Decode, Encode};
use cumulus_zombienet_sdk_helpers::{
	open_hrmp_channel, submit_extrinsic_and_wait_for_finalization_success_with_timeout,
};
use serde_json::json;
use sp_consensus_aura::{sr25519::AuthorityId, ConsensusLog, AURA_ENGINE_ID};
use sp_core::crypto::key_types;
use std::{collections::BTreeSet, str::FromStr, time::Duration};
use zombienet_sdk::{
	subxt::{
		self,
		config::substrate::DigestItem,
		dynamic::Value,
		ext::{
			scale_encode::EncodeAsType,
			scale_value::{At, Composite, ValueDef},
		},
		utils::H256,
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::{
		sr25519::{dev, Keypair},
		SecretUri,
	},
	NetworkConfig, NetworkConfigBuilder,
};

pub(super) const ASSET_HUB_ID: u32 = 1000;
pub(super) const PEOPLE_ID: u32 = 1004;

pub(super) const ASSET_HUB_INVULNERABLE: &str = "asset-hub-collator";
pub(super) const PEOPLE_INVULNERABLE: &str = "people-collator";

/// Length of the unsigned election phase on Asset Hub, in blocks.
const UNSIGNED_PHASE: u32 = 20;

/// For each sudo call on the relay chain and each RPC request to a collator node.
pub(super) const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// From the parachain-block wait ending to the relay chain session change that opens the HRMP
/// channels. Relay chain sessions last 2 minutes.
pub(super) const HRMP_OPEN_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// Blocks each parachain must finalize before the test changes anything.
pub(super) const PARA_BLOCKS: u32 = 3;
pub(super) const PARA_BLOCKS_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(6);

/// For connecting a client to a node that is up.
pub(super) const CLIENT_TIMEOUT_SECS: u64 = 60;

/// For zombienet to spawn the whole network, and each node.
const NETWORK_SPAWN_TIMEOUT_SECS: u32 = 10 * 60;
const NODE_SPAWN_TIMEOUT_SECS: u32 = 5 * 60;

/// A relay chain validator: the relay node, its stash and its collator node on each parachain.
pub(super) struct Validator {
	pub(super) stash_uri: &'static str,
	pub(super) asset_hub_collator: &'static str,
	pub(super) people_collator: &'static str,
}

pub(super) const VALIDATORS: [Validator; 2] = [
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

/// What the network adds to `westend-local` with validators `alice` and `bob`,
/// `people-westend-local` and optionally `asset-hub-westend-local`, each parachain with one
/// invulnerable collator.
pub(super) struct NetworkOptions {
	/// Spawns Asset Hub. Without it the network has People only.
	pub(super) asset_hub: bool,
	/// One collator node per validator on each parachain, not a collator at genesis.
	pub(super) validator_collators: bool,
	/// `pallet-staking-async-ah-client` in `Active` mode, Asset Hub electing the two validators,
	/// the signed election phases skipped and a short unsigned phase. Asset Hub starts without
	/// the preset's generated dev stakers either way.
	pub(super) election: bool,
	/// Overrides the relay chain's `hrmp_channel_max_total_size`.
	pub(super) hrmp_channel_max_total_size: Option<u32>,
	/// Balance of Alice, the relay chain's sudo key, in place of zombienet's default.
	pub(super) sudo_balance: Option<u128>,
	/// `ValidatorCollators::ReportPeriod` of every spawned parachain at genesis, in blocks, in
	/// place of the runtime default.
	pub(super) report_period: Option<u32>,
	/// `ValidatorCollators::ExpectedEraDuration` of every spawned parachain at genesis, in
	/// milliseconds, in place of the runtime default.
	pub(super) expected_era_duration: Option<u64>,
}

/// Logging from `RUST_LOG`, `info` by default. The orchestrator's chain spec generator is capped
/// at `debug`, since at trace it dumps the whole raw chain spec while merging a raw spec override.
pub(super) fn init_logging() {
	let _ = env_logger::Builder::from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	)
	.filter_module("zombienet_orchestrator::generators::chain_spec", log::LevelFilter::Debug)
	.try_init();
}

pub(super) async fn build_network_config(
	options: NetworkOptions,
) -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	let mut relay_overrides = serde_json::Map::new();
	if options.election {
		relay_overrides.insert("stakingAhClient".into(), json!({ "operatingMode": "Active" }));
	}
	if let Some(total_size) = options.hrmp_channel_max_total_size {
		relay_overrides.insert(
			"configuration".into(),
			json!({ "config": { "hrmp_channel_max_total_size": total_size } }),
		);
	}
	let extra_collators = if options.validator_collators { &VALIDATORS[..] } else { &[] };
	let report_settings = [
		options
			.report_period
			.map(|period| validator_collators_entry("ReportPeriod", period)),
		options
			.expected_era_duration
			.map(|duration| validator_collators_entry("ExpectedEraDuration", duration)),
	]
	.into_iter()
	.flatten();
	let asset_hub_raw = report_settings
		.clone()
		.chain(options.election.then(election_phase_entries).into_iter().flatten())
		.collect::<Vec<_>>();
	let people_raw = report_settings.collect::<Vec<_>>();

	let network = NetworkConfigBuilder::new().with_relaychain(|r| {
		let r = r
			.with_chain("westend-local")
			.with_default_command("polkadot")
			.with_default_image(images.polkadot.as_str())
			.with_default_args(vec![
				("-lparachain=info,runtime::staking-async::ah-client=debug,xcm=info").into(),
			]);
		// The chain spec generator rejects an empty override.
		let r =
			if relay_overrides.is_empty() { r } else { r.with_genesis_overrides(relay_overrides) };
		r.with_validator(|node| match options.sudo_balance {
			Some(balance) => node.with_name("alice").with_initial_balance(balance),
			None => node.with_name("alice"),
		})
		.with_validator(|node| node.with_name("bob"))
	});
	let network = if options.asset_hub {
		network.with_parachain(|p| {
			let p = p
				.with_id(ASSET_HUB_ID)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("asset-hub-westend-local")
				.with_default_args(collator_args(
					"runtime::multiblock-election=debug,runtime::staking-async=debug,\
					runtime::staking-async::rc-client=debug,runtime::validator-collators=debug,\
					runtime::validator-set-announcer=debug",
				));
			let p = if options.election {
				p.with_genesis_overrides(json!({
					"staking": { "validatorCount": VALIDATORS.len(), "devStakers": null }
				}))
			} else {
				p.with_genesis_overrides(json!({ "staking": { "devStakers": null } }))
			};
			let p = if asset_hub_raw.is_empty() {
				p
			} else {
				p.with_raw_spec_override(raw_genesis(asset_hub_raw))
			};
			extra_collators.iter().fold(
				p.with_collator(|n| n.with_name(ASSET_HUB_INVULNERABLE)),
				|p, validator| {
					p.with_collator(|n| {
						n.with_name(validator.asset_hub_collator).invulnerable(false)
					})
				},
			)
		})
	} else {
		network
	};
	network
		.with_parachain(|p| {
			let p = p
				.with_id(PEOPLE_ID)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("people-westend-local")
				.with_default_args(collator_args("runtime::validator-collators=debug"));
			let p = if people_raw.is_empty() {
				p
			} else {
				p.with_raw_spec_override(raw_genesis(people_raw))
			};
			extra_collators.iter().fold(
				p.with_collator(|n| n.with_name(PEOPLE_INVULNERABLE)),
				|p, validator| {
					p.with_collator(|n| n.with_name(validator.people_collator).invulnerable(false))
				},
			)
		})
		.with_global_settings(|global_settings| {
			let global_settings = global_settings
				.with_network_spawn_timeout(NETWORK_SPAWN_TIMEOUT_SECS)
				.with_node_spawn_timeout(NODE_SPAWN_TIMEOUT_SECS);
			match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
				Ok(val) => global_settings.with_base_dir(val),
				_ => global_settings,
			}
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

/// A raw genesis override writing these storage entries.
fn raw_genesis(entries: Vec<(Vec<u8>, Vec<u8>)>) -> serde_json::Value {
	let top = serde_json::Map::from_iter(entries.into_iter().map(|(key, value)| {
		(format!("0x{}", hex::encode(key)), json!(format!("0x{}", hex::encode(value))))
	}));
	json!({ "genesis": { "raw": { "top": top } } })
}

/// Skips the signed election phases on Asset Hub and shortens the unsigned one, through the
/// `pub storage` parameters `SignedPhase`, `UnsignedPhase` and `SignedValidationPhase`.
fn election_phase_entries() -> Vec<(Vec<u8>, Vec<u8>)> {
	let entry = |name: &str, value: u32| {
		(sp_crypto_hashing::twox_128(format!(":{name}:").as_bytes()).to_vec(), value.encode())
	};
	vec![
		entry("SignedPhase", 0),
		entry("UnsignedPhase", UNSIGNED_PHASE),
		entry("SignedValidationPhase", 0),
	]
}

/// The storage value `item` of `ValidatorCollators`.
fn validator_collators_entry(item: &str, value: impl Encode) -> (Vec<u8>, Vec<u8>) {
	let key = [
		sp_crypto_hashing::twox_128(b"ValidatorCollators"),
		sp_crypto_hashing::twox_128(item.as_bytes()),
	]
	.concat();
	(key, value.encode())
}

/// Opens the HRMP channels from Asset Hub to People and back, taking messages of up to
/// `max_message_size` bytes. They open at the next relay chain session change. The channel back
/// lets People answer Asset Hub's version subscription.
pub(super) async fn open_hrmp_channels(
	relay_client: &OnlineClient<PolkadotConfig>,
	max_message_size: u32,
) -> Result<(), anyhow::Error> {
	for (sender, recipient) in [(ASSET_HUB_ID, PEOPLE_ID), (PEOPLE_ID, ASSET_HUB_ID)] {
		// `max_capacity` is the preset's `hrmp_channel_max_capacity`.
		open_hrmp_channel(
			relay_client,
			sender,
			recipient,
			8,
			max_message_size,
			&dev::alice(),
			CALL_TIMEOUT.as_secs(),
		)
		.await?;
		log::info!("Requested HRMP channel {sender} to {recipient}");
	}
	Ok(())
}

pub(super) async fn wait_for_hrmp_channel(
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

/// Waits for an event of `pallet` and `variant` whose fields satisfy `matches`, and returns the
/// number of the finalized block holding it.
pub(super) async fn wait_for_event(
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

/// `AnnouncementSent { destination: People, era: 1 }`.
pub(super) fn is_announcement_of_era_1(fields: &Composite<u32>) -> bool {
	let to_people = fields.at("destination").is_some_and(
		|destination| matches!(&destination.value, ValueDef::Variant(v) if v.name == "People"),
	);
	to_people && fields.at("era").and_then(|era| era.as_u128()) == Some(1)
}

pub(super) async fn wait_for_finalized_block(
	client: &OnlineClient<PolkadotConfig>,
	chain: &str,
	number: u32,
	timeout: Duration,
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
	tokio::time::timeout(timeout, wait)
		.await
		.map_err(|_| anyhow!("{chain} did not finalize #{number} within {timeout:?}"))?
}

/// A parachain under test.
pub(super) struct Chain<'a> {
	pub(super) name: &'static str,
	pub(super) id: u32,
	pub(super) client: &'a OnlineClient<PolkadotConfig>,
	pub(super) invulnerable: &'static str,
}

/// Raw storage keys and values for `System::set_storage`.
pub(super) type StorageItems = Vec<(Vec<u8>, Vec<u8>)>;

/// The block that enacts a new Aura authority list.
pub(super) struct RotationBlock {
	pub(super) hash: H256,
	pub(super) number: u32,
	pub(super) header_size: usize,
	pub(super) authorities: usize,
}

/// A raw storage item of `pallet` and `item` at `keys`, with the key hashed and the value
/// encoded as the chain's metadata describes them.
pub(super) fn storage_item(
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
pub(super) fn session_key_items(
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

/// Sends `call` from the relay chain to `chain`, which dispatches it as root.
pub(super) async fn transact_as_root(
	relay_client: &OnlineClient<PolkadotConfig>,
	chain: &Chain<'_>,
	call: subxt::tx::DynamicPayload,
) -> Result<(), anyhow::Error> {
	let name = format!("{}::{}", call.pallet_name(), call.call_name());
	let call = chain.client.tx().call_data(&call)?;
	submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		relay_client,
		&root_transact(chain.id, call),
		&dev::alice(),
		CALL_TIMEOUT.as_secs(),
	)
	.await
	.map_err(|e| anyhow!("sending `{name}` to {}: {e}", chain.name))?;
	log::info!("Sent `{name}` to {}", chain.name);
	Ok(())
}

/// `Sudo::sudo(XcmPallet::send)` of an unpaid `Transact` of `call` to parachain `para_id`, which
/// the parachain dispatches as root.
pub(super) fn root_transact(para_id: u32, call: Vec<u8>) -> subxt::tx::DynamicPayload {
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

pub(super) async fn current_index(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<u32, anyhow::Error> {
	Ok(fetch(client, "Session", "CurrentIndex", vec![]).await?.unwrap_or_default())
}

/// Waits for a finalized block whose header carries Aura's `AuthoritiesChange` digest. Only
/// finalized blocks count, since the collator may build blocks that do not become canonical.
pub(super) async fn wait_for_authorities_change(
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

pub(super) async fn session_validators(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<BTreeSet<[u8; 32]>, anyhow::Error> {
	let validators: Vec<[u8; 32]> =
		fetch(client, "Session", "Validators", vec![]).await?.unwrap_or_default();
	Ok(validators.into_iter().collect())
}

pub(super) async fn fetch<T: Decode>(
	client: &OnlineClient<PolkadotConfig>,
	pallet: &str,
	item: &str,
	keys: Vec<Value>,
) -> Result<Option<T>, anyhow::Error> {
	let at = client.backend().latest_finalized_block_ref().await?.hash();
	fetch_at(client, at, pallet, item, keys).await
}

pub(super) async fn fetch_at<T: Decode>(
	client: &OnlineClient<PolkadotConfig>,
	at: H256,
	pallet: &str,
	item: &str,
	keys: Vec<Value>,
) -> Result<Option<T>, anyhow::Error> {
	fetch_raw_at(client, at, pallet, item, keys)
		.await?
		.map(|encoded| {
			T::decode(&mut &encoded[..]).map_err(|e| anyhow!("decode `{pallet}::{item}`: {e}"))
		})
		.transpose()
}

pub(super) async fn fetch_raw(
	client: &OnlineClient<PolkadotConfig>,
	pallet: &str,
	item: &str,
	keys: Vec<Value>,
) -> Result<Option<Vec<u8>>, anyhow::Error> {
	let at = client.backend().latest_finalized_block_ref().await?.hash();
	fetch_raw_at(client, at, pallet, item, keys).await
}

pub(super) async fn fetch_raw_at(
	client: &OnlineClient<PolkadotConfig>,
	at: H256,
	pallet: &str,
	item: &str,
	keys: Vec<Value>,
) -> Result<Option<Vec<u8>>, anyhow::Error> {
	let query = subxt::dynamic::storage(pallet, item, keys);
	let value = client.storage().at(at).fetch(&query).await?;
	Ok(value.map(|value| value.into_encoded()))
}

pub(super) fn keypair(uri: &str) -> Result<Keypair, anyhow::Error> {
	Ok(Keypair::from_uri(&SecretUri::from_str(uri)?)?)
}

pub(super) fn account(keypair: &Keypair) -> [u8; 32] {
	keypair.public_key().0
}

/// The seed zombienet derives a node's keys from: its name with the first letter capitalised.
pub(super) fn node_seed(name: &str) -> String {
	let mut chars = name.chars();
	let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
	format!("//{first}{}", chars.as_str())
}

pub(super) fn hex_accounts(accounts: &BTreeSet<[u8; 32]>) -> String {
	accounts
		.iter()
		.map(|a| format!("0x{}", hex::encode(a)))
		.collect::<Vec<_>>()
		.join(", ")
}
