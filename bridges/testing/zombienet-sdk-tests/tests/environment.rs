// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Shared environment for the Rococo <> Westend bridge zombienet-sdk tests.
//!
//! This is the Rust port of `bridges/testing/environments/rococo-westend/*` (the legacy
//! `spawn.sh`, `bridges_rococo_westend.sh`, `start_relayer.sh` and the `framework/js-helpers`).
//! It:
//!   * spawns two relay-chain networks (Rococo and Westend), each with a Bridge Hub (para 1013 /
//!     1002) and an Asset Hub (para 1000) collator,
//!   * initializes the bridge (HRMP channels, remote XCM versions, asset-conversion pools and
//!     funding of the sovereign/reward accounts),
//!   * drives the external `substrate-relay` binary as a set of subprocesses, and
//!   * exposes `subxt` query/extrinsic helpers used by the individual tests.

use anyhow::anyhow;
use codec::Decode;
use std::{future::Future, path::PathBuf, time::Duration};
use subxt::{tx::Payload, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::{dev, Keypair};
use tokio::{
	process::{Child, Command},
	time::{sleep, timeout_at, Instant},
};
use zombienet_sdk::{
	environment::get_spawn_fn, Arg, LocalFileSystem, Network, NetworkConfig, NetworkConfigBuilder,
};

// `1u64 << 60` — the amount the local chain specs endow every well-known account with. The
// finality/parachain relayers (`//Charlie` / `//Dave`) only submit free or mandatory headers, so
// their balance must stay exactly at this value throughout the tests.
pub const ENDOWMENT: u128 = 1u128 << 60;

/// Message lane shared by both Asset Hubs (`0x00000002`).
pub const LANE_ID: [u8; 4] = [0, 0, 0, 2];
/// Target XCM version configured for every remote location.
pub const XCM_VERSION: u32 = 5;

// Bridged chain ids (`*b"bhwd"` / `*b"bhro"`), used as reward keys on the Bridge Hubs.
pub const BRIDGED_CHAIN_ID_BHWD: [u8; 4] = *b"bhwd";
pub const BRIDGED_CHAIN_ID_BHRO: [u8; 4] = *b"bhro";

// `6408de77..` / `e143f238..` — genesis hashes used as `NetworkId::ByGenesis(..)`.
pub const ROCOCO_GENESIS_HASH: [u8; 32] = [
	100, 8, 222, 119, 55, 197, 156, 35, 136, 144, 83, 58, 242, 88, 150, 162, 194, 6, 8, 216, 179,
	128, 187, 1, 2, 154, 203, 57, 39, 129, 6, 62,
];
pub const WESTEND_GENESIS_HASH: [u8; 32] = [
	225, 67, 242, 56, 3, 172, 80, 232, 246, 248, 230, 38, 149, 209, 206, 158, 78, 29, 104, 170, 54,
	193, 205, 44, 253, 21, 52, 2, 19, 243, 66, 62,
];

// Para ids.
pub const ASSET_HUB_PARA_ID: u32 = 1000;
pub const BRIDGE_HUB_ROCOCO_PARA_ID: u32 = 1013;
pub const BRIDGE_HUB_WESTEND_PARA_ID: u32 = 1002;

// Sovereign / reward accounts funded on the Bridge Hubs (see `bridges_rococo_westend.sh`).
const ASSET_HUB_SOVEREIGN_AT_BRIDGE_HUB: &str = "5Eg2fntNprdN3FgH4sfEaaZhYtddZQSQUqvYJ1f2mLtinVhV";
const BHR_LANE_THIS_CHAIN: &str = "5EHnXaT5GApse1euZWj9hycMbgjKBCNQL9WEwScL8QDx6mhK";
const BHR_LANE_BRIDGED_CHAIN: &str = "5EHnXaT5Tnt4A8aiP9CsuAFRhKPjKZJXRrj4a3mtihFvKpTi";
const BHW_LANE_THIS_CHAIN: &str = "5EHnXaT5GApry9tS6yd1FVusPq8o8bQJGCKyvXTFCoEKk5Z9";
const BHW_LANE_BRIDGED_CHAIN: &str = "5EHnXaT5Tnt3VGpEvc6jSgYwVToDGxLRMuYoZ8coo6GHyWbR";

// The amount funded onto each sovereign/reward account.
const SOVEREIGN_FUNDING: u128 = 100_000_000_000_000;

// ---------------------------------------------------------------------------------------------
// Per-runtime typed operations.
//
// Rococo/Westend (relays) and the two Asset Hubs / Bridge Hubs share the same type layout for the
// calls and storage we touch, but `subxt` generates a distinct module per runtime, so the types
// are nominally different. The macros below generate the (identical) bodies once per runtime.
// ---------------------------------------------------------------------------------------------

macro_rules! relay_ops {
	($name:ident, $relay:ident, $runtime:ident) => {
		pub mod $name {
			use super::sign_submit_wait;
			use crate::$relay::runtime_types::{
				pallet_xcm::pallet::Call as XcmPalletCall,
				polkadot_parachain_primitives::primitives::Id,
				polkadot_runtime_parachains::hrmp::pallet::Call as HrmpCall,
				sp_weights::weight_v2::Weight,
				staging_xcm::v4::{
					junction::Junction, junctions::Junctions, location::Location, Instruction, Xcm,
				},
				xcm::{
					double_encoded::DoubleEncoded,
					v3::{OriginKind, WeightLimit},
					VersionedLocation, VersionedXcm,
				},
				$runtime::RuntimeCall,
			};
			use subxt::{OnlineClient, PolkadotConfig};
			use subxt_signer::sr25519::Keypair;

			/// `sudo(Hrmp::force_open_hrmp_channel(..))`.
			pub async fn open_hrmp_channel(
				client: &OnlineClient<PolkadotConfig>,
				sudo: &Keypair,
				sender: u32,
				recipient: u32,
				max_capacity: u32,
				max_message_size: u32,
			) -> Result<(), anyhow::Error> {
				let call = RuntimeCall::Hrmp(HrmpCall::force_open_hrmp_channel {
					sender: Id(sender),
					recipient: Id(recipient),
					max_capacity,
					max_message_size,
				});
				let tx = crate::$relay::tx().sudo().sudo(call);
				sign_submit_wait(client, &tx, sudo).await
			}

			/// `sudo(XcmPallet::send(..))` carrying an `UnpaidExecution` + `Transact{Superuser}`
			/// message to the given parachain — the governance primitive used to configure the
			/// system parachains, mirroring `send_governance_transact` from the bash framework.
			pub async fn send_governance_transact(
				client: &OnlineClient<PolkadotConfig>,
				sudo: &Keypair,
				para_id: u32,
				encoded_call: Vec<u8>,
				require_weight_ref_time: u64,
				require_weight_proof_size: u64,
			) -> Result<(), anyhow::Error> {
				let dest = VersionedLocation::V4(Location {
					parents: 0,
					interior: Junctions::X1([Junction::Parachain(para_id)]),
				});
				let message = VersionedXcm::V4(Xcm(vec![
					Instruction::UnpaidExecution {
						weight_limit: WeightLimit::Unlimited,
						check_origin: None,
					},
					Instruction::Transact {
						origin_kind: OriginKind::Superuser,
						require_weight_at_most: Weight {
							ref_time: require_weight_ref_time,
							proof_size: require_weight_proof_size,
						},
						call: DoubleEncoded { encoded: encoded_call },
					},
				]));
				let call = RuntimeCall::XcmPallet(XcmPalletCall::send {
					dest: Box::new(dest),
					message: Box::new(message),
				});
				let tx = crate::$relay::tx().sudo().sudo(call);
				sign_submit_wait(client, &tx, sudo).await
			}
		}
	};
}

macro_rules! asset_hub_ops {
	($name:ident, $ah:ident) => {
		pub mod $name {
			use super::{free_balance_at, sign_submit_wait};
			use crate::$ah::runtime_types::{
				staging_xcm::v5::{
					asset::{Asset, AssetId, Assets, Fungibility},
					junction::{Junction, NetworkId},
					junctions::Junctions,
					location::Location,
				},
				xcm::{v3::WeightLimit, VersionedAssets, VersionedLocation},
			};
			use subxt::{tx::Payload, OnlineClient, PolkadotConfig};
			use subxt_signer::sr25519::Keypair;

			/// The local native asset, `{ parents: 1, interior: Here }`.
			pub fn native_asset() -> Location {
				Location { parents: 1, interior: Junctions::Here }
			}

			/// A bridged native asset, `{ parents: 2, interior: X1(GlobalConsensus(by_genesis)) }`.
			pub fn bridged_asset(by_genesis: [u8; 32]) -> Location {
				Location {
					parents: 2,
					interior: Junctions::X1([Junction::GlobalConsensus(NetworkId::ByGenesis(
						by_genesis,
					))]),
				}
			}

			/// The remote Asset Hub location, `{ parents: 2, X2(GlobalConsensus, Parachain(1000))
			/// }`.
			pub fn remote_asset_hub(by_genesis: [u8; 32]) -> Location {
				Location {
					parents: 2,
					interior: Junctions::X2([
						Junction::GlobalConsensus(NetworkId::ByGenesis(by_genesis)),
						Junction::Parachain(super::ASSET_HUB_PARA_ID),
					]),
				}
			}

			/// `tx.assetConversion.createPool(native, bridged)`.
			pub async fn create_pool(
				client: &OnlineClient<PolkadotConfig>,
				signer: &Keypair,
				bridged_by_genesis: [u8; 32],
			) -> Result<(), anyhow::Error> {
				let tx = crate::$ah::tx()
					.asset_conversion()
					.create_pool(native_asset(), bridged_asset(bridged_by_genesis));
				sign_submit_wait(client, &tx, signer).await
			}

			/// `tx.assetConversion.addLiquidity(native, bridged, ..)`.
			pub async fn add_liquidity(
				client: &OnlineClient<PolkadotConfig>,
				signer: &Keypair,
				bridged_by_genesis: [u8; 32],
				native_amount: u128,
				bridged_amount: u128,
				mint_to: subxt::utils::AccountId32,
			) -> Result<(), anyhow::Error> {
				let tx = crate::$ah::tx().asset_conversion().add_liquidity(
					native_asset(),
					bridged_asset(bridged_by_genesis),
					native_amount,
					bridged_amount,
					1,
					1,
					mint_to,
				);
				sign_submit_wait(client, &tx, signer).await
			}

			/// `tx.polkadotXcm.limitedReserveTransferAssets(..)` from this Asset Hub to the remote
			/// one, sending `amount` of `asset` to `beneficiary` (an `AccountId32`).
			pub async fn limited_reserve_transfer(
				client: &OnlineClient<PolkadotConfig>,
				signer: &Keypair,
				remote_by_genesis: [u8; 32],
				beneficiary: [u8; 32],
				asset: Location,
				amount: u128,
			) -> Result<(), anyhow::Error> {
				let dest = VersionedLocation::V5(remote_asset_hub(remote_by_genesis));
				let beneficiary = VersionedLocation::V5(Location {
					parents: 0,
					interior: Junctions::X1([Junction::AccountId32 {
						network: None,
						id: beneficiary,
					}]),
				});
				let assets = VersionedAssets::V5(Assets(vec![Asset {
					id: AssetId(asset),
					fun: Fungibility::Fungible(amount),
				}]));
				let tx = crate::$ah::tx().polkadot_xcm().limited_reserve_transfer_assets(
					dest,
					beneficiary,
					assets,
					0,
					WeightLimit::Unlimited,
				);
				sign_submit_wait(client, &tx, signer).await
			}

			/// SCALE-encoded `PolkadotXcm::force_xcm_version(remote, version)` call, to be wrapped
			/// in a relay-chain governance `Transact`.
			pub async fn force_xcm_version_call(
				client: &OnlineClient<PolkadotConfig>,
				remote: Location,
				version: u32,
			) -> Result<Vec<u8>, anyhow::Error> {
				let call = crate::$ah::tx().polkadot_xcm().force_xcm_version(remote, version);
				Ok(call.encode_call_data(&client.metadata())?)
			}

			/// Free balance of `account` (native asset) via `system.account`.
			pub async fn free_balance(
				client: &OnlineClient<PolkadotConfig>,
				account: [u8; 32],
			) -> Result<u128, anyhow::Error> {
				free_balance_at(client, account).await
			}

			/// Balance of a bridged (foreign) asset held by `account`, or `None` if the account
			/// has no entry for that asset yet. Mirrors `wrapped-assets-balance.js`.
			pub async fn foreign_asset_balance(
				client: &OnlineClient<PolkadotConfig>,
				asset: Location,
				account: subxt::utils::AccountId32,
			) -> Result<Option<u128>, anyhow::Error> {
				let addr = crate::$ah::storage().foreign_assets().account(asset, account);
				let maybe = client.storage().at_latest().await?.fetch(&addr).await?;
				Ok(maybe.map(|a| a.balance))
			}

			/// Whether the HRMP egress channel towards `sibling` is open. Mirrors
			/// `wait-hrmp-channel-opened.js`.
			pub async fn hrmp_egress_open(
				client: &OnlineClient<PolkadotConfig>,
				sibling: u32,
			) -> Result<bool, anyhow::Error> {
				let addr = crate::$ah::storage().parachain_system().relevant_messaging_state();
				let Some(state) = client.storage().at_latest().await?.fetch(&addr).await? else {
					return Ok(false);
				};
				Ok(state.egress_channels.iter().any(|(id, _)| id.0 == sibling))
			}
		}
	};
}

macro_rules! bridge_hub_ops {
	($name:ident, $bh:ident) => {
		pub mod $name {
			use super::{free_balance_at, sign_submit_wait, XCM_VERSION};
			use crate::$bh::runtime_types::staging_xcm::v5::{
				junction::{Junction, NetworkId},
				junctions::Junctions,
				location::Location,
			};
			use subxt::{tx::Payload, OnlineClient, PolkadotConfig};
			use subxt_signer::sr25519::Keypair;

			/// The remote Bridge Hub location, `{ parents: 2, X2(GlobalConsensus, Parachain) }`.
			pub fn remote_bridge_hub(by_genesis: [u8; 32], para: u32) -> Location {
				Location {
					parents: 2,
					interior: Junctions::X2([
						Junction::GlobalConsensus(NetworkId::ByGenesis(by_genesis)),
						Junction::Parachain(para),
					]),
				}
			}

			/// `tx.balances.transferAllowDeath(target, amount)`.
			pub async fn transfer_balance(
				client: &OnlineClient<PolkadotConfig>,
				signer: &Keypair,
				target: subxt::utils::AccountId32,
				amount: u128,
			) -> Result<(), anyhow::Error> {
				let tx = crate::$bh::tx()
					.balances()
					.transfer_allow_death(subxt::utils::MultiAddress::Id(target), amount);
				sign_submit_wait(client, &tx, signer).await
			}

			/// SCALE-encoded `PolkadotXcm::force_xcm_version(remote, XCM_VERSION)` call.
			pub async fn force_xcm_version_call(
				client: &OnlineClient<PolkadotConfig>,
				remote: Location,
			) -> Result<Vec<u8>, anyhow::Error> {
				let call = crate::$bh::tx().polkadot_xcm().force_xcm_version(remote, XCM_VERSION);
				Ok(call.encode_call_data(&client.metadata())?)
			}

			/// Free balance of `account` via `system.account`.
			pub async fn free_balance(
				client: &OnlineClient<PolkadotConfig>,
				account: [u8; 32],
			) -> Result<u128, anyhow::Error> {
				free_balance_at(client, account).await
			}
		}
	};
}

relay_ops!(relay_rococo, rococo, rococo_runtime);
relay_ops!(relay_westend, westend, westend_runtime);
asset_hub_ops!(asset_hub_rococo, asset_hub_rococo);
asset_hub_ops!(asset_hub_westend, asset_hub_westend);
bridge_hub_ops!(bridge_hub_rococo, bridge_hub_rococo);
bridge_hub_ops!(bridge_hub_westend, bridge_hub_westend);

/// Reads `bridgeRelayers.relayerRewards(relayer, RewardsAccountParams)` on Bridge Hub Rococo.
pub async fn bridge_hub_rococo_relayer_reward(
	client: &OnlineClient<PolkadotConfig>,
	relayer: subxt::utils::AccountId32,
) -> Result<Option<u128>, anyhow::Error> {
	use crate::bridge_hub_rococo::runtime_types::{
		bp_messages::lane::LegacyLaneId,
		bp_relayers::{RewardsAccountOwner, RewardsAccountParams},
	};
	let reward = RewardsAccountParams {
		owner: RewardsAccountOwner::ThisChain,
		bridged_chain_id: BRIDGED_CHAIN_ID_BHWD,
		lane_id: LegacyLaneId(LANE_ID),
	};
	let addr = crate::bridge_hub_rococo::storage()
		.bridge_relayers()
		.relayer_rewards(relayer, reward);
	Ok(client.storage().at_latest().await?.fetch(&addr).await?)
}

/// Reads `bridgeRelayers.relayerRewards(relayer, BridgeReward::RococoWestend(..))` on Bridge Hub
/// Westend (the reward kind is wrapped in the runtime's `BridgeReward` enum there).
pub async fn bridge_hub_westend_relayer_reward(
	client: &OnlineClient<PolkadotConfig>,
	relayer: subxt::utils::AccountId32,
) -> Result<Option<u128>, anyhow::Error> {
	use crate::bridge_hub_westend::runtime_types::{
		bp_messages::lane::LegacyLaneId,
		bp_relayers::{RewardsAccountOwner, RewardsAccountParams},
		bridge_hub_westend_runtime::bridge_common_config::BridgeReward,
	};
	let reward = BridgeReward::RococoWestend(RewardsAccountParams {
		owner: RewardsAccountOwner::ThisChain,
		bridged_chain_id: BRIDGED_CHAIN_ID_BHRO,
		lane_id: LegacyLaneId(LANE_ID),
	});
	let addr = crate::bridge_hub_westend::storage()
		.bridge_relayers()
		.relayer_rewards(relayer, reward);
	Ok(client.storage().at_latest().await?.fetch(&addr).await?)
}

// ---------------------------------------------------------------------------------------------
// Generic subxt helpers (runtime-agnostic).
// ---------------------------------------------------------------------------------------------

/// Signs `call` with `signer` (default params), submits it and waits for finalized success.
pub async fn sign_submit_wait<C: Payload>(
	client: &OnlineClient<PolkadotConfig>,
	call: &C,
	signer: &Keypair,
) -> Result<(), anyhow::Error> {
	client
		.tx()
		.sign_and_submit_then_watch_default(call, signer)
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(())
}

/// Free balance of `account` via dynamic `System::Account` storage (works for any runtime).
pub async fn free_balance_at(
	client: &OnlineClient<PolkadotConfig>,
	account: [u8; 32],
) -> Result<u128, anyhow::Error> {
	use subxt::ext::scale_value::{At, Value};
	let addr = subxt::dynamic::storage("System", "Account", vec![Value::from_bytes(account)]);
	let Some(value) = client.storage().at_latest().await?.fetch(&addr).await? else {
		return Ok(0);
	};
	let value = value.to_value()?;
	value
		.at("data")
		.and_then(|data| data.at("free"))
		.and_then(|free| free.as_u128())
		.ok_or_else(|| anyhow!("unexpected System::Account layout"))
}

/// Calls the `<Chain>FinalityApi_best_finalized` runtime API and returns the best finalized
/// bridged header number (mirrors `best-finalized-header-at-bridged-chain.js`).
pub async fn best_finalized_bridged_header(
	client: &OnlineClient<PolkadotConfig>,
	finality_api: &str,
) -> Result<Option<u32>, anyhow::Error> {
	let method = format!("{finality_api}_best_finalized");
	let encoded = client.runtime_api().at_latest().await?.call_raw(method.as_str(), None).await?;
	// `Option<HeaderId<Hash, Number>>` where `HeaderId(Number, Hash)` — we only need the number.
	let decoded: Option<(u32, [u8; 32])> = Decode::decode(&mut &encoded[..])?;
	Ok(decoded.map(|(number, _hash)| number))
}

/// Waits until `client` reports a finalized block of at least `height`, or `timeout` elapses.
pub async fn wait_for_block_height(
	client: &OnlineClient<PolkadotConfig>,
	height: u32,
	timeout: Duration,
) -> Result<(), anyhow::Error> {
	let mut sub = client.blocks().subscribe_finalized().await?;
	let deadline = Instant::now() + timeout;
	while let Ok(Some(block)) = timeout_at(deadline, sub.next()).await {
		if block?.number() >= height {
			return Ok(());
		}
	}
	Err(anyhow!("timeout waiting for block height {height}"))
}

/// Subscribes to finalized blocks of `client` for `duration` and counts the GRANDPA
/// (`UpdatedBestFinalizedHeader`) and parachain (`UpdatedParachainHead`) header-import events
/// emitted by the given bridge pallets (mirrors `multiple-headers-synced.js`).
pub async fn count_synced_headers(
	client: &OnlineClient<PolkadotConfig>,
	grandpa_pallet: &str,
	parachains_pallet: &str,
	duration: Duration,
) -> Result<(u32, u32), anyhow::Error> {
	let mut sub = client.blocks().subscribe_finalized().await?;
	let deadline = Instant::now() + duration;
	let (mut grandpa_headers, mut parachain_headers) = (0u32, 0u32);
	while let Ok(Some(block)) = timeout_at(deadline, sub.next()).await {
		let block = block?;
		for event in block.events().await?.iter() {
			let event = event?;
			match (event.pallet_name(), event.variant_name()) {
				(p, "UpdatedBestFinalizedHeader") if p == grandpa_pallet => grandpa_headers += 1,
				(p, "UpdatedParachainHead") if p == parachains_pallet => parachain_headers += 1,
				_ => {},
			}
		}
	}
	Ok((grandpa_headers, parachain_headers))
}

/// Polls `f` every 6s until it yields `Some`, or `timeout` elapses.
pub async fn retry_until<F, Fut, T>(timeout: Duration, mut f: F) -> Result<T, anyhow::Error>
where
	F: FnMut() -> Fut,
	Fut: Future<Output = Result<Option<T>, anyhow::Error>>,
{
	let deadline = Instant::now() + timeout;
	loop {
		if let Some(value) = f().await? {
			return Ok(value);
		}
		if Instant::now() >= deadline {
			return Err(anyhow!("timeout in retry_until"));
		}
		sleep(Duration::from_secs(6)).await;
	}
}

// ---------------------------------------------------------------------------------------------
// `substrate-relay` subprocess driver.
// ---------------------------------------------------------------------------------------------

const RELAYER_RUST_LOG: &str = "runtime=trace,rpc=trace,bridge=trace";

fn relayer_binary() -> PathBuf {
	if let Ok(path) = std::env::var("SUBSTRATE_RELAY_BINARY") {
		return PathBuf::from(path);
	}
	let home = std::env::var("HOME").unwrap_or_default();
	PathBuf::from(home).join("local_bridge_testing/bin/substrate-relay")
}

/// A spawned long-running `substrate-relay` process. Killed when dropped.
pub struct Relayer(Child);

impl Drop for Relayer {
	fn drop(&mut self) {
		let _ = self.0.start_kill();
	}
}

fn spawn_relayer(args: &[&str]) -> Result<Relayer, anyhow::Error> {
	log::info!("Spawning substrate-relay {}", args.join(" "));
	let child = Command::new(relayer_binary())
		.args(args)
		.env("RUST_LOG", RELAYER_RUST_LOG)
		.kill_on_drop(true)
		.spawn()?;
	Ok(Relayer(child))
}

async fn run_relayer_to_completion(args: &[&str]) -> Result<(), anyhow::Error> {
	log::info!("Running substrate-relay {}", args.join(" "));
	let status = Command::new(relayer_binary())
		.args(args)
		.env("RUST_LOG", RELAYER_RUST_LOG)
		.status()
		.await?;
	if !status.success() {
		return Err(anyhow!("substrate-relay {:?} exited with {status}", args));
	}
	Ok(())
}

// ---------------------------------------------------------------------------------------------
// Network spawning.
// ---------------------------------------------------------------------------------------------

/// The full Rococo <> Westend bridge environment: both networks plus any running relayer
/// processes (kept alive for the lifetime of the value).
pub struct BridgeTestEnv {
	pub rococo: Network<LocalFileSystem>,
	pub westend: Network<LocalFileSystem>,
	_relayers: Vec<Relayer>,
}

fn rococo_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let bh_args: Vec<Arg> =
		vec!["-lparachain=debug,runtime::bridge=trace,xcm=trace,txpool=trace".into()];
	let ah_args: Vec<Arg> =
		vec!["-lparachain=debug,xcm=trace,runtime::bridge=trace,txpool=trace".into()];
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug,xcm=trace".into()])
				.with_validator(|n| {
					n.with_name("alice-rococo-validator")
						.with_rpc_port(9942)
						.with_initial_balance(2_000_000_000_000)
				})
				.with_validator(|n| {
					n.with_name("bob-rococo-validator")
						.with_rpc_port(9943)
						.with_initial_balance(2_000_000_000_000)
				})
				.with_validator(|n| {
					n.with_name("charlie-rococo-validator")
						.with_rpc_port(9944)
						.with_initial_balance(2_000_000_000_000)
				})
		})
		.with_parachain(|p| {
			p.with_id(BRIDGE_HUB_ROCOCO_PARA_ID)
				.with_chain("bridge-hub-rococo-local")
				.cumulus_based(true)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("bridge-hub-rococo-collator1")
						.with_rpc_port(8943)
						.with_args(bh_args.clone())
				})
		})
		.with_parachain(|p| {
			p.with_id(ASSET_HUB_PARA_ID)
				.with_chain("asset-hub-rococo-local")
				.cumulus_based(true)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("asset-hub-rococo-collator1")
						.with_rpc_port(9910)
						.with_args(ah_args.clone())
				})
		})
		.with_global_settings(global_settings)
		.build()
		.map_err(config_errs)
}

fn westend_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let bh_args: Vec<Arg> =
		vec!["-lparachain=debug,runtime::bridge=trace,xcm=trace,txpool=trace".into()];
	let ah_args: Vec<Arg> = vec![
		"-lparachain=debug,xcm=trace,runtime::bridge=trace,txpool=trace".into(),
		"--authoring".into(),
		"slot-based".into(),
	];
	NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug,xcm=trace".into()])
				.with_validator(|n| {
					n.with_name("alice-westend-validator")
						.with_rpc_port(9945)
						.with_initial_balance(2_000_000_000_000)
				})
				.with_validator(|n| {
					n.with_name("bob-westend-validator")
						.with_rpc_port(9946)
						.with_initial_balance(2_000_000_000_000)
				})
				.with_validator(|n| {
					n.with_name("charlie-westend-validator")
						.with_rpc_port(9947)
						.with_initial_balance(2_000_000_000_000)
				})
		})
		.with_parachain(|p| {
			p.with_id(BRIDGE_HUB_WESTEND_PARA_ID)
				.with_chain("bridge-hub-westend-local")
				.cumulus_based(true)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("bridge-hub-westend-collator1")
						.with_rpc_port(8945)
						.with_args(bh_args.clone())
				})
		})
		.with_parachain(|p| {
			p.with_id(ASSET_HUB_PARA_ID)
				.with_chain("asset-hub-westend-local")
				.cumulus_based(true)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_collator(|n| {
					n.with_name("asset-hub-westend-collator1")
						.with_rpc_port(9010)
						.with_args(ah_args.clone())
				})
		})
		.with_global_settings(global_settings)
		.build()
		.map_err(config_errs)
}

/// Shared global settings for both networks. We disable `tear_down_on_failure` so the background
/// node-monitoring task (which declares a node "crashed" if its metrics endpoint does not respond
/// within ~5s) does not tear the network down on a transient, load-induced timeout — the same
/// approach the polkadot zombienet-sdk tests use on busy CI runners.
fn global_settings(
	settings: zombienet_sdk::GlobalSettingsBuilder,
) -> zombienet_sdk::GlobalSettingsBuilder {
	// `with_node_spawn_timeout` takes seconds (zombienet's own `Duration` alias, i.e. `u32`).
	settings.with_tear_down_on_failure(false).with_node_spawn_timeout(600)
}

fn config_errs(errs: Vec<anyhow::Error>) -> anyhow::Error {
	anyhow!(
		"network config errors: {}",
		errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
	)
}

impl BridgeTestEnv {
	/// Spawns both networks and, depending on the flags, initializes the bridge and starts the
	/// relayer. Mirrors `spawn.sh [--init] [--start-relayer]`.
	pub async fn spawn(init: bool, start_relayer: bool) -> Result<Self, anyhow::Error> {
		let _ = env_logger::try_init_from_env(
			env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
		);

		let spawn_fn = get_spawn_fn();
		log::info!("Spawning Rococo network");
		let rococo = spawn_fn(rococo_network_config()?).await?;
		log::info!("Spawning Westend network");
		let westend = spawn_fn(westend_network_config()?).await?;

		let mut env = BridgeTestEnv { rococo, westend, _relayers: Vec::new() };

		if init {
			env.init_bridge().await?;
		}
		if start_relayer {
			env.start_relayer().await?;
		}
		Ok(env)
	}

	async fn client_of(
		network: &Network<LocalFileSystem>,
		node: &str,
	) -> Result<OnlineClient<PolkadotConfig>, anyhow::Error> {
		let node = network.get_node(node)?;
		let client: OnlineClient<PolkadotConfig> = node.wait_client().await?;
		Ok(client)
	}

	pub async fn rococo_relay_client(&self) -> Result<OnlineClient<PolkadotConfig>, anyhow::Error> {
		Self::client_of(&self.rococo, "alice-rococo-validator").await
	}
	pub async fn westend_relay_client(
		&self,
	) -> Result<OnlineClient<PolkadotConfig>, anyhow::Error> {
		Self::client_of(&self.westend, "alice-westend-validator").await
	}
	pub async fn asset_hub_rococo_client(
		&self,
	) -> Result<OnlineClient<PolkadotConfig>, anyhow::Error> {
		Self::client_of(&self.rococo, "asset-hub-rococo-collator1").await
	}
	pub async fn asset_hub_westend_client(
		&self,
	) -> Result<OnlineClient<PolkadotConfig>, anyhow::Error> {
		Self::client_of(&self.westend, "asset-hub-westend-collator1").await
	}
	pub async fn bridge_hub_rococo_client(
		&self,
	) -> Result<OnlineClient<PolkadotConfig>, anyhow::Error> {
		Self::client_of(&self.rococo, "bridge-hub-rococo-collator1").await
	}
	pub async fn bridge_hub_westend_client(
		&self,
	) -> Result<OnlineClient<PolkadotConfig>, anyhow::Error> {
		Self::client_of(&self.westend, "bridge-hub-westend-collator1").await
	}

	/// Initializes both sides of the bridge: waits for block production, opens HRMP channels, sets
	/// remote XCM versions, creates the asset-conversion pools and funds the sovereign / reward
	/// accounts. Mirrors the `--init` path of `spawn.sh`.
	async fn init_bridge(&self) -> Result<(), anyhow::Error> {
		let rococo_relay = self.rococo_relay_client().await?;
		let westend_relay = self.westend_relay_client().await?;
		let ahr = self.asset_hub_rococo_client().await?;
		let ahw = self.asset_hub_westend_client().await?;
		let bhr = self.bridge_hub_rococo_client().await?;
		let bhw = self.bridge_hub_westend_client().await?;

		let alice = dev::alice();
		let bob = dev::bob();
		let bob_account = dev::bob().public_key().to_account_id();

		// rococo-start / westend-start: parachains produce blocks reliably.
		log::info!("Waiting for parachains to start producing blocks");
		wait_for_block_height(&ahr, 10, Duration::from_secs(180)).await?;
		wait_for_block_height(&bhr, 10, Duration::from_secs(180)).await?;
		wait_for_block_height(&ahw, 10, Duration::from_secs(180)).await?;
		wait_for_block_height(&bhw, 10, Duration::from_secs(180)).await?;

		// init-rococo-local / init-westend-local: HRMP channels + remote XCM versions.
		log::info!("Opening HRMP channels and setting remote XCM versions");
		relay_rococo::open_hrmp_channel(
			&rococo_relay,
			&alice,
			ASSET_HUB_PARA_ID,
			BRIDGE_HUB_ROCOCO_PARA_ID,
			4,
			524288,
		)
		.await?;
		relay_rococo::open_hrmp_channel(
			&rococo_relay,
			&alice,
			BRIDGE_HUB_ROCOCO_PARA_ID,
			ASSET_HUB_PARA_ID,
			4,
			524288,
		)
		.await?;
		relay_westend::open_hrmp_channel(
			&westend_relay,
			&alice,
			ASSET_HUB_PARA_ID,
			BRIDGE_HUB_WESTEND_PARA_ID,
			4,
			524288,
		)
		.await?;
		relay_westend::open_hrmp_channel(
			&westend_relay,
			&alice,
			BRIDGE_HUB_WESTEND_PARA_ID,
			ASSET_HUB_PARA_ID,
			4,
			524288,
		)
		.await?;

		// Remote XCM versions (Asset Hub <-> Asset Hub, Bridge Hub <-> Bridge Hub).
		let ahw_on_ahr = asset_hub_rococo::remote_asset_hub(WESTEND_GENESIS_HASH);
		let force_ahw =
			asset_hub_rococo::force_xcm_version_call(&ahr, ahw_on_ahr, XCM_VERSION).await?;
		relay_rococo::send_governance_transact(
			&rococo_relay,
			&alice,
			ASSET_HUB_PARA_ID,
			force_ahw,
			200_000_000,
			12_000,
		)
		.await?;

		let bhw_on_bhr =
			bridge_hub_rococo::remote_bridge_hub(WESTEND_GENESIS_HASH, BRIDGE_HUB_WESTEND_PARA_ID);
		let force_bhw = bridge_hub_rococo::force_xcm_version_call(&bhr, bhw_on_bhr).await?;
		relay_rococo::send_governance_transact(
			&rococo_relay,
			&alice,
			BRIDGE_HUB_ROCOCO_PARA_ID,
			force_bhw,
			200_000_000,
			12_000,
		)
		.await?;

		let ahr_on_ahw = asset_hub_westend::remote_asset_hub(ROCOCO_GENESIS_HASH);
		let force_ahr =
			asset_hub_westend::force_xcm_version_call(&ahw, ahr_on_ahw, XCM_VERSION).await?;
		relay_westend::send_governance_transact(
			&westend_relay,
			&alice,
			ASSET_HUB_PARA_ID,
			force_ahr,
			200_000_000,
			12_000,
		)
		.await?;

		let bhr_on_bhw =
			bridge_hub_westend::remote_bridge_hub(ROCOCO_GENESIS_HASH, BRIDGE_HUB_ROCOCO_PARA_ID);
		let force_bhr = bridge_hub_westend::force_xcm_version_call(&bhw, bhr_on_bhw).await?;
		relay_westend::send_governance_transact(
			&westend_relay,
			&alice,
			BRIDGE_HUB_WESTEND_PARA_ID,
			force_bhr,
			200_000_000,
			12_000,
		)
		.await?;

		// rococo-init / westend-init: HRMP channels are open.
		log::info!("Waiting for HRMP channels to open");
		retry_until(Duration::from_secs(600), || {
			let ahr = ahr.clone();
			async move {
				Ok(asset_hub_rococo::hrmp_egress_open(&ahr, BRIDGE_HUB_ROCOCO_PARA_ID)
					.await?
					.then_some(()))
			}
		})
		.await?;
		retry_until(Duration::from_secs(600), || {
			let ahw = ahw.clone();
			async move {
				Ok(asset_hub_westend::hrmp_egress_open(&ahw, BRIDGE_HUB_WESTEND_PARA_ID)
					.await?
					.then_some(()))
			}
		})
		.await?;

		// init-asset-hub-*-local: asset-conversion pools + liquidity.
		log::info!("Creating asset-conversion pools and adding liquidity");
		asset_hub_rococo::create_pool(&ahr, &bob, WESTEND_GENESIS_HASH).await?;
		asset_hub_rococo::add_liquidity(
			&ahr,
			&bob,
			WESTEND_GENESIS_HASH,
			1_000_000_000_000,
			2_500_000_000_000,
			bob_account.clone(),
		)
		.await?;
		asset_hub_westend::create_pool(&ahw, &bob, ROCOCO_GENESIS_HASH).await?;
		asset_hub_westend::add_liquidity(
			&ahw,
			&bob,
			ROCOCO_GENESIS_HASH,
			1_000_000_000_000,
			4_000_000_000_000,
			bob_account,
		)
		.await?;

		// init-bridge-hub-*-local: fund sovereign / reward accounts.
		log::info!("Funding sovereign and reward accounts on the Bridge Hubs");
		for account in
			[ASSET_HUB_SOVEREIGN_AT_BRIDGE_HUB, BHR_LANE_THIS_CHAIN, BHR_LANE_BRIDGED_CHAIN]
		{
			bridge_hub_rococo::transfer_balance(
				&bhr,
				&alice,
				parse_account(account)?,
				SOVEREIGN_FUNDING,
			)
			.await?;
		}
		for account in
			[ASSET_HUB_SOVEREIGN_AT_BRIDGE_HUB, BHW_LANE_THIS_CHAIN, BHW_LANE_BRIDGED_CHAIN]
		{
			bridge_hub_westend::transfer_balance(
				&bhw,
				&alice,
				parse_account(account)?,
				SOVEREIGN_FUNDING,
			)
			.await?;
		}

		log::info!("Bridge initialization complete");
		Ok(())
	}

	/// Initializes the GRANDPA bridge pallets and starts the finality, parachains and messages
	/// relayers. Mirrors `start_relayer.sh` + the relayer commands in `bridges_rococo_westend.sh`.
	pub async fn start_relayer(&mut self) -> Result<(), anyhow::Error> {
		// init-bridge (one-off, blocking).
		run_relayer_to_completion(&[
			"init-bridge",
			"westend-to-bridge-hub-rococo",
			"--source-uri",
			"ws://localhost:9945",
			"--source-version-mode",
			"Auto",
			"--target-uri",
			"ws://localhost:8943",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Bob",
		])
		.await?;
		run_relayer_to_completion(&[
			"init-bridge",
			"rococo-to-bridge-hub-westend",
			"--source-uri",
			"ws://localhost:9942",
			"--source-version-mode",
			"Auto",
			"--target-uri",
			"ws://localhost:8945",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Bob",
		])
		.await?;

		// Finality relayers (free relay-chain headers, signed by //Charlie).
		self._relayers.push(spawn_relayer(&[
			"relay-headers",
			"rococo-to-bridge-hub-westend",
			"--only-free-headers",
			"--source-uri",
			"ws://localhost:9942",
			"--source-version-mode",
			"Auto",
			"--target-uri",
			"ws://localhost:8945",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Charlie",
			"--target-transactions-mortality",
			"4",
		])?);
		self._relayers.push(spawn_relayer(&[
			"relay-headers",
			"westend-to-bridge-hub-rococo",
			"--only-free-headers",
			"--source-uri",
			"ws://localhost:9945",
			"--source-version-mode",
			"Auto",
			"--target-uri",
			"ws://localhost:8943",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Charlie",
			"--target-transactions-mortality",
			"4",
		])?);

		// Parachains relayers (free parachain headers, signed by //Dave).
		self._relayers.push(spawn_relayer(&[
			"relay-parachains",
			"bridge-hub-rococo-to-bridge-hub-westend",
			"--only-free-headers",
			"--source-uri",
			"ws://localhost:9942",
			"--source-version-mode",
			"Auto",
			"--target-uri",
			"ws://localhost:8945",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Dave",
			"--target-transactions-mortality",
			"4",
		])?);
		self._relayers.push(spawn_relayer(&[
			"relay-parachains",
			"bridge-hub-westend-to-bridge-hub-rococo",
			"--only-free-headers",
			"--source-uri",
			"ws://localhost:9945",
			"--source-version-mode",
			"Auto",
			"--target-uri",
			"ws://localhost:8943",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Dave",
			"--target-transactions-mortality",
			"4",
		])?);

		// Messages relayers (lane 0x00000002; //Eve for ro->wnd, //Ferdie for wnd->ro).
		self._relayers.push(spawn_relayer(&[
			"relay-messages",
			"bridge-hub-rococo-to-bridge-hub-westend",
			"--source-uri",
			"ws://localhost:8943",
			"--source-version-mode",
			"Auto",
			"--source-signer",
			"//Eve",
			"--source-transactions-mortality",
			"4",
			"--target-uri",
			"ws://localhost:8945",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Eve",
			"--target-transactions-mortality",
			"4",
			"--lane",
			"00000002",
		])?);
		self._relayers.push(spawn_relayer(&[
			"relay-messages",
			"bridge-hub-westend-to-bridge-hub-rococo",
			"--source-uri",
			"ws://localhost:8945",
			"--source-version-mode",
			"Auto",
			"--source-signer",
			"//Ferdie",
			"--source-transactions-mortality",
			"4",
			"--target-uri",
			"ws://localhost:8943",
			"--target-version-mode",
			"Auto",
			"--target-signer",
			"//Ferdie",
			"--target-transactions-mortality",
			"4",
			"--lane",
			"00000002",
		])?);

		// rococo-bridge / westend-bridge: wait until the GRANDPA pallets are initialized.
		log::info!("Waiting for the GRANDPA bridge pallets to be initialized");
		let bhr = self.bridge_hub_rococo_client().await?;
		let bhw = self.bridge_hub_westend_client().await?;
		retry_until(Duration::from_secs(400), || {
			let bhr = bhr.clone();
			async move {
				Ok(best_finalized_bridged_header(&bhr, "WestendFinalityApi")
					.await?
					.filter(|n| *n > 0)
					.map(|_| ()))
			}
		})
		.await?;
		retry_until(Duration::from_secs(400), || {
			let bhw = bhw.clone();
			async move {
				Ok(best_finalized_bridged_header(&bhw, "RococoFinalityApi")
					.await?
					.filter(|n| *n > 0)
					.map(|_| ()))
			}
		})
		.await?;
		log::info!("Relayer started and GRANDPA bridge pallets initialized");
		Ok(())
	}
}

/// Account id of a dev signer, as a `subxt` `AccountId32`.
pub fn dev_account(keypair: &Keypair) -> subxt::utils::AccountId32 {
	subxt::utils::AccountId32(keypair.public_key().0)
}

/// 32-byte public key of a dev signer.
pub fn dev_public(keypair: &Keypair) -> [u8; 32] {
	keypair.public_key().0
}

fn parse_account(ss58: &str) -> Result<subxt::utils::AccountId32, anyhow::Error> {
	ss58.parse::<subxt::utils::AccountId32>()
		.map_err(|e| anyhow!("invalid SS58 account {ss58}: {e:?}"))
}
