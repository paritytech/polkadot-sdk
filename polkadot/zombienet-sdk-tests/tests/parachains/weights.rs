// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate::helpers::asset_hub_westend::{
	self,
	runtime_types::{
		pallet_revive::primitives::{Code, StorageDeposit},
		sp_weights::weight_v2::Weight,
	},
};
use anyhow::anyhow;
use futures::{stream::FuturesUnordered, StreamExt};
use pallet_revive::AddressMapper;
use sp_core::{H160, H256};
use std::str::FromStr;
use subxt::{tx::SubmittableExtrinsic, OnlineClient, PolkadotConfig};
use subxt_signer::{
	sr25519::{dev, Keypair},
	SecretUri,
};
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder};

const KEYS_COUNT: usize = 3000;
const CHUNK_SIZE: usize = 500;
const CALL_CHUNK_SIZE: usize = 1000;

#[tokio::test(flavor = "multi_thread")]
async fn weights_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let network = setup_network().await?;
	let collator = network.get_node("collator")?;
	let para_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
	let mut call_clients = vec![];
	for _ in 0..(KEYS_COUNT / CALL_CHUNK_SIZE) {
		let call_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
		call_clients.push(call_client);
	}
	log::info!("Network is ready");

	let alice = dev::alice();
	let keys = create_keys(KEYS_COUNT);
	setup_accounts(&para_client, &alice, &keys).await?;
	log::info!("Accounts ready");

	let contract_address = instantiate_contract(&para_client, &alice).await?;
	log::info!("Contract instantiated: {:?}", contract_address);

	call_contract(&para_client, call_clients, contract_address, &alice, &keys).await?;
	log::info!("Test finished, sleeping for 6000 seconds to allow for manual inspection");
	tokio::time::sleep(std::time::Duration::from_secs(6000)).await;

	Ok(())
}

async fn setup_network() -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("validator-0"))
				.with_node(|node| node.with_name("validator-1"))
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("polkadot-parachain")
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.with_chain("asset-hub-westend-local")
				.with_collator(|n| {
					n.with_name("collator").validator(true).with_args(vec![
						// ("--force-in-memory-trie-cache").into(),
						("-linfo").into(),
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

	Ok(network)
}

fn create_keys(n: usize) -> Vec<Keypair> {
	(0..n)
		.map(|i| {
			let uri = SecretUri::from_str(&format!("//key{}", i)).unwrap();
			Keypair::from_uri(&uri).unwrap()
		})
		.collect()
}

async fn setup_accounts(
	para_client: &OnlineClient<PolkadotConfig>,
	alice: &Keypair,
	keys: &[Keypair],
) -> Result<(), anyhow::Error> {
	let alice_account_id = alice.public_key().to_account_id();
	for chunk in keys.chunks(CHUNK_SIZE) {
		let alice_nonce = para_client.tx().account_nonce(&alice_account_id).await?;
		let txs = chunk
			.iter()
			.enumerate()
			.map(|(i, key)| {
				let params = subxt::config::polkadot::PolkadotExtrinsicParamsBuilder::new()
					.nonce(alice_nonce + i as u64)
					.build();
				para_client.tx().create_signed_offline(
					&asset_hub_westend::tx()
						.balances()
						.transfer_keep_alive(key.public_key().into(), 1000000000000),
					alice,
					params,
				)
			})
			.collect::<Result<Vec<_>, _>>()?;
		submit_txs(txs).await?;
	}
	log::info!("Sleeping for 30 seconds to finalize the transfer");
	tokio::time::sleep(std::time::Duration::from_secs(30)).await;

	let alice_nonce = para_client.tx().account_nonce(&alice_account_id).await?;
	let params = subxt::config::polkadot::PolkadotExtrinsicParamsBuilder::new()
		.nonce(alice_nonce)
		.build();
	para_client
		.tx()
		.sign_and_submit_then_watch(&asset_hub_westend::tx().revive().map_account(), alice, params)
		.await?
		.wait_for_finalized_success()
		.await?;

	for chunk in keys.chunks(CHUNK_SIZE) {
		let txs = chunk
			.iter()
			.map(|key| {
				let params =
					subxt::config::polkadot::PolkadotExtrinsicParamsBuilder::new().nonce(0).build();
				para_client.tx().create_signed_offline(
					&asset_hub_westend::tx().revive().map_account(),
					key,
					params,
				)
			})
			.collect::<Result<Vec<_>, _>>()?;
		submit_txs(txs).await?;
	}
	log::info!("Sleeping for 30 seconds to finalize the mapping");
	tokio::time::sleep(std::time::Duration::from_secs(30)).await;

	Ok(())
}

async fn instantiate_contract(
	para_client: &OnlineClient<PolkadotConfig>,
	alice: &Keypair,
) -> Result<H160, anyhow::Error> {
	let code_path = std::env::current_dir().unwrap().join("tests/parachains/contract.polkavm");
	let code = std::fs::read(code_path)?;
	let alice_account_id = alice.public_key().to_account_id();
	let alice_h160 =
		<asset_hub_westend_runtime::Runtime as pallet_revive::Config>::AddressMapper::to_address(
			&alice.public_key().0.into(),
		);

	let upload_dry_run = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(asset_hub_westend::apis().revive_api().upload_code(
			alice_account_id.clone(),
			code.clone(),
			None,
		))
		.await?
		.unwrap();
	let code_hash = upload_dry_run.code_hash;
	let deposit = upload_dry_run.deposit;
	para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().upload_code(code.clone(), deposit),
			alice,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	let instantiate_dry_run = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(asset_hub_westend::apis().revive_api().instantiate(
			alice_account_id.clone(),
			0,
			None,
			None,
			Code::Existing(code_hash),
			vec![],
			None,
		))
		.await?;
	// We need a nonce before instantiating the contract
	let alice_revive_nonce = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(asset_hub_westend::apis().revive_api().nonce(alice_h160))
		.await?;
	let contract_address = pallet_revive::create1(&alice_h160, alice_revive_nonce.into());
	let weight = Weight {
		ref_time: instantiate_dry_run.gas_required.ref_time,
		proof_size: instantiate_dry_run.gas_required.proof_size,
	};
	let deposit = match instantiate_dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};
	para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().instantiate(
				0,
				weight,
				deposit,
				code_hash,
				vec![],
				None,
			),
			alice,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	Ok(contract_address)
}

async fn call_contract(
	para_client: &OnlineClient<PolkadotConfig>,
	mut call_clients: Vec<OnlineClient<PolkadotConfig>>,
	contract_address: H160,
	alice: &Keypair,
	keys: &[Keypair],
) -> Result<(), anyhow::Error> {
	let mint_100 = sp_core::hex2array!(
		"a0712d680000000000000000000000000000000000000000000000000000000000000064"
	)
	.to_vec();
	let alice_account_id = alice.public_key().to_account_id();
	let call_dry_run = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(asset_hub_westend::apis().revive_api().call(
			alice_account_id.clone(),
			contract_address,
			0,
			None,
			None,
			mint_100.clone(),
		))
		.await?;
	let deposit = match call_dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};
	let mut txs = vec![];
	for chunk in keys.chunks(CALL_CHUNK_SIZE) {
		let para_client = call_clients.pop().unwrap();
		let txs_chunk = chunk
			.iter()
			.map(|key| {
				let weight = Weight {
					ref_time: call_dry_run.gas_required.ref_time,
					proof_size: call_dry_run.gas_required.proof_size,
				};
				let params =
					subxt::config::polkadot::PolkadotExtrinsicParamsBuilder::new().nonce(1).build();
				para_client.tx().create_signed_offline(
					&asset_hub_westend::tx().revive().call(
						contract_address,
						0,
						weight,
						deposit,
						mint_100.clone(),
					),
					key,
					params,
				)
			})
			.collect::<Result<Vec<_>, _>>()?;
		txs.extend(txs_chunk);
	}
	let finalized_blocks = submit_txs(txs).await?;
	for block in finalized_blocks {
		let weight = para_client
			.storage()
			.at(block)
			.fetch(&asset_hub_westend::storage().system().block_weight())
			.await?;
		log::info!("Weight of block {:?}: {:?}", block, weight);
	}

	Ok(())
}

async fn submit_txs(
	txs: Vec<SubmittableExtrinsic<PolkadotConfig, OnlineClient<PolkadotConfig>>>,
) -> Result<std::collections::HashSet<H256>, anyhow::Error> {
	let futs = txs.iter().map(|tx| tx.submit_and_watch()).collect::<FuturesUnordered<_>>();
	let res = futs.collect::<Vec<_>>().await;
	let res: Result<Vec<_>, _> = res.into_iter().collect();
	let res = res.expect("All the transactions submitted successfully");
	let mut statuses = futures::stream::select_all(res);
	let mut finalized_blocks = std::collections::HashSet::new();
	while let Some(a) = statuses.next().await {
		match a {
			Ok(st) => match st {
				subxt::tx::TxStatus::Validated => log::trace!("VALIDATED"),
				subxt::tx::TxStatus::Broadcasted { num_peers } =>
					log::trace!("BROADCASTED TO {num_peers}"),
				subxt::tx::TxStatus::NoLongerInBestBlock => log::warn!("NO LONGER IN BEST BLOCK"),
				subxt::tx::TxStatus::InBestBlock(_) => log::trace!("IN BEST BLOCK"),
				subxt::tx::TxStatus::InFinalizedBlock(block) => {
					log::trace!("IN FINALIZED BLOCK");
					finalized_blocks.insert(block.block_hash());
				},
				subxt::tx::TxStatus::Error { message } => log::warn!("ERROR: {message}"),
				subxt::tx::TxStatus::Invalid { message } => log::trace!("INVALID: {message}"),
				subxt::tx::TxStatus::Dropped { message } => log::trace!("DROPPED: {message}"),
			},
			Err(e) => {
				println!("Error status {:?}", e);
			},
		}
	}
	Ok(finalized_blocks)
}
