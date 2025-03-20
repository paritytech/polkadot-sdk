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

const KEYS_COUNT: usize = 100;
const CHUNK_SIZE: usize = 500;
const CALL_CHUNK_SIZE: usize = 1000;

#[tokio::test(flavor = "multi_thread")]
async fn weights_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// run(TrieCacheSetup::Default, 100).await?;
	run(TrieCacheSetup::InMemory, 100).await?;
	// run(TrieCacheSetup::Default, 1).await?;
	// run(TrieCacheSetup::InMemory, 1).await?;

	Ok(())
}

#[derive(Debug, Clone, Copy)]
enum TrieCacheSetup {
	InMemory,
	Default,
}

async fn run(setup: TrieCacheSetup, accounts_count: usize) -> Result<(), anyhow::Error> {
	log::info!("Running with setup: {:?}, accounts_count: {:?}", setup, accounts_count);
	let mint_100 = sp_core::hex2array!(
		"a0712d680000000000000000000000000000000000000000000000000000000000000064"
	)
	.to_vec();
	let network = setup_network(setup).await?;
	let collator = network.get_node("collator")?;
	let para_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
	log::info!("Network is ready");

	let alice = dev::alice();
	let keys = create_keys(accounts_count);
	setup_accounts(&para_client, &alice, &keys).await?;
	log::info!("Accounts ready");

	let contract_address = instantiate_contract(&para_client, &alice).await?;
	log::info!("Contract instantiated: {:?}", contract_address);
	let params = (499999999, 99999, 29999999999);
	// contract_params(&para_client, contract_address, mint_100.clone(), &alice).await?;
	// log::info!("Params: {:?}", params);

	let finalized =
		call_contract(&para_client, contract_address, params, mint_100.clone(), &keys, 1).await?;
	// let hit_rate = find_shared_cache_hit_rate(collator.logs().await?, finalized).await?;
	// log::info!("Shared cache hit rate: {:?} %", hit_rate);
	tokio::time::sleep(std::time::Duration::from_secs(30)).await;
	collator.restart(None).await?;
	log::info!("Restarted");
	let para_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
	// let params = contract_params(&para_client, contract_address, mint_100.clone(),
	// &alice).await?; log::info!("Params: {:?}", params);
	let finalized =
		call_contract(&para_client, contract_address, params, mint_100.clone(), &keys, 2).await?;
	let hit_rate = find_shared_cache_hit_rate(collator.logs().await?, finalized).await?;
	log::info!("Shared cache hit rate: {:?} %", hit_rate);

	// network.destroy().await?;

	Ok(())
}

async fn setup_network(setup: TrieCacheSetup) -> Result<Network<LocalFileSystem>, anyhow::Error> {
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
					n.with_name("collator").validator(true).with_args(match setup {
						TrieCacheSetup::InMemory => vec![
							("--force-in-memory-trie-cache").into(),
							("-linfo,trie-cache=debug").into(),
						],
						TrieCacheSetup::Default => vec![("-linfo,trie-cache=debug").into()],
					})
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

async fn contract_params(
	para_client: &OnlineClient<PolkadotConfig>,
	contract_address: H160,
	payload: Vec<u8>,
	alice: &Keypair,
) -> Result<(u64, u64, u128), anyhow::Error> {
	let alice_account_id = alice.public_key().to_account_id();
	let call_dry_run = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(asset_hub_westend::apis().revive_api().call(
			alice_account_id,
			contract_address,
			0,
			None,
			None,
			payload,
		))
		.await?;
	let deposit = match call_dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	Ok((call_dry_run.gas_required.ref_time, call_dry_run.gas_required.proof_size, deposit))
}

async fn call_contract(
	para_client: &OnlineClient<PolkadotConfig>,
	contract_address: H160,
	params: (u64, u64, u128),
	payload: Vec<u8>,
	keys: &[Keypair],
	nonce: u64,
) -> Result<H256, anyhow::Error> {
	let mut txs = vec![];
	let (ref_time, proof_size, deposit) = params;
	for chunk in keys.chunks(CALL_CHUNK_SIZE) {
		let txs_chunk = chunk
			.iter()
			.map(|key| {
				let params = subxt::config::polkadot::PolkadotExtrinsicParamsBuilder::new()
					.nonce(nonce)
					.build();
				para_client.tx().create_signed_offline(
					&asset_hub_westend::tx().revive().call(
						contract_address,
						0,
						Weight { ref_time, proof_size },
						deposit,
						payload.clone(),
					),
					key,
					params,
				)
			})
			.collect::<Result<Vec<_>, _>>()?;
		txs.extend(txs_chunk);
	}
	let blocks = submit_txs(txs).await?;
	let mut block_weights = vec![];
	for block in blocks {
		let weight = para_client
			.storage()
			.at(block)
			.fetch(&asset_hub_westend::storage().system().block_weight())
			.await?
			.unwrap();
		log::info!("Weight of block {:?}: {:?}", block, weight);
		block_weights.push((block, weight.normal.ref_time));
	}
	log::info!("Got weights");
	let most_filled_block = block_weights.iter().max_by_key(|(_, ref_time)| ref_time).unwrap().0;
	Ok(most_filled_block)
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

async fn find_shared_cache_hit_rate(logs: String, block: H256) -> Result<u32, anyhow::Error> {
	let logs = logs.lines().collect::<Vec<_>>();
	// Looking for the log "🎁 Prepared block for proposing at XXX (XXX ms)," which appears before
	// the first log that mentions the block hash.
	let index = logs
		.iter()
		.rev()
		.position(|l| l.contains("Local node identity is:"))
		.ok_or_else(|| anyhow!("Log not found"))?;
	let hit_rates = logs[(logs.len() - index)..]
		.iter()
		.filter(|l| l.contains("[Parachain]") && l.contains("shared hit rate ="))
		.map(|l| {
			// Extract score for shared cache from a log which ends like this:
			// "trie cache dropped: local hit rate = 0% [0/3], shared hit rate = 100% [3/3]"
			let score = l
				.split_whitespace()
				.last()
				.unwrap()
				.split("/")
				.map(|s| s.trim_matches(|c| c == '[' || c == ']').parse::<u32>().unwrap())
				.collect::<Vec<_>>();
			assert_eq!(score.len(), 2);
			(score[0], score[1])
		})
		.collect::<Vec<_>>();

	let mut hit = 0;
	let mut total = 0;
	for rate in hit_rates {
		if total > 100 {
			break;
		}
		hit += rate.0;
		total += rate.1;
	}
	log::info!("Hit: {:?}, Total: {:?}", hit, total);
	let hit_rate = hit as f64 / total as f64 * 100.0;

	Ok(hit_rate as u32)
}
