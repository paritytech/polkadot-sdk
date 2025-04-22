// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// TODO: Remove this metadata and use "metadata-files/asset-hub-westend-local.scale" after metadata
// on master is same as on the asset-hub-westend
#[subxt::subxt(runtime_metadata_path = "tests/parachains/asset-hub-westend-local.metadata")]
mod asset_hub_westend {}

use anyhow::anyhow;
use asset_hub_westend::runtime_types::{
	pallet_revive::primitives::{Code, StorageDeposit},
	sp_weights::weight_v2::Weight,
};
use futures::{stream::FuturesUnordered, StreamExt};
use pallet_revive::AddressMapper;
use sp_core::H160;
use std::str::FromStr;
use subxt::{
	config::polkadot::PolkadotExtrinsicParamsBuilder, tx::SubmittableExtrinsic, OnlineClient,
	PolkadotConfig,
};
use subxt_signer::{
	sr25519::{dev, Keypair},
	SecretUri,
};
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn warm_up_trie_cache_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let hit_rate = run(100).await?;
	assert!(hit_rate > 90);

	Ok(())
}

async fn run(accounts_count: usize) -> Result<u32, anyhow::Error> {
	let network = setup_network().await?;
	let collator = network.get_node("collator")?;
	tokio::time::sleep(std::time::Duration::from_secs(3)).await;
	let para_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
	log::info!("Network is ready");

	let alice = dev::alice();
	let keys = create_keys(accounts_count);
	let mut nonce = 0;
	let mut nonce = || {
		let current_nonce = nonce;
		nonce += 1;
		current_nonce
	};

	setup_accounts(&para_client, &alice, &keys, nonce()).await?;
	log::info!("Accounts ready");

	let contract_address = instantiate_contract(&para_client, &alice).await?;
	log::info!("Contract instantiated: {:?}", contract_address);

	call_contract(&para_client, contract_address, &alice, &keys, nonce()).await?;
	log::info!("Contract called first time");

	collator.restart(None).await?;
	tokio::time::sleep(std::time::Duration::from_secs(3)).await;
	let para_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
	log::info!("Collator restarted");

	call_contract(&para_client, contract_address, &alice, &keys, nonce()).await?;
	log::info!("Contract called second time");

	let hit_rate = find_shared_cache_hit_rate(collator.logs().await?).await?;
	log::info!("Shared cache hit rate: {:?} %", hit_rate);

	Ok(hit_rate)
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
						("--warm-up-trie-cache").into(),
						("-linfo,trie-cache=debug").into(),
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
	caller: &Keypair,
	keys: &[Keypair],
	nonce: u64,
) -> Result<(), anyhow::Error> {
	let caller_account_id = caller.public_key().to_account_id();
	let caller_nonce = para_client.tx().account_nonce(&caller_account_id).await?;
	let transfers = keys
		.iter()
		.enumerate()
		.map(|(i, key)| {
			let params =
				PolkadotExtrinsicParamsBuilder::new().nonce(caller_nonce + i as u64).build();
			para_client.tx().create_signed_offline(
				&asset_hub_westend::tx()
					.balances()
					.transfer_keep_alive(key.public_key().into(), 1000000000000),
				caller,
				params,
			)
		})
		.collect::<Result<Vec<_>, _>>()?;
	submit_txs(transfers).await?;

	let mut mappings = keys
		.iter()
		.map(|key| {
			para_client.tx().create_signed_offline(
				&asset_hub_westend::tx().revive().map_account(),
				key,
				PolkadotExtrinsicParamsBuilder::new().nonce(nonce).build(),
			)
		})
		.collect::<Result<Vec<_>, _>>()?;
	let caller_nonce = para_client.tx().account_nonce(&caller_account_id).await?;
	let caller_mapping = para_client.tx().create_signed_offline(
		&asset_hub_westend::tx().revive().map_account(),
		caller,
		PolkadotExtrinsicParamsBuilder::new().nonce(caller_nonce).build(),
	)?;
	mappings.push(caller_mapping);
	submit_txs(mappings).await?;

	Ok(())
}

async fn call_params(
	para_client: &OnlineClient<PolkadotConfig>,
	contract_address: H160,
	payload: Vec<u8>,
	caller: &Keypair,
) -> Result<(u64, u64, u128), anyhow::Error> {
	let caller_account_id = caller.public_key().to_account_id();
	let call = asset_hub_westend::apis().revive_api().call(
		caller_account_id,
		contract_address,
		0,
		None,
		None,
		payload,
	);
	let dry_run = para_client.runtime_api().at_latest().await?.call(call).await?;
	let deposit = match dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	Ok((dry_run.gas_required.ref_time, dry_run.gas_required.proof_size, deposit))
}

async fn instantiate_params(
	para_client: &OnlineClient<PolkadotConfig>,
	code: Vec<u8>,
	caller: &Keypair,
) -> Result<(u64, u64, u128), anyhow::Error> {
	let caller_account_id = caller.public_key().to_account_id();
	let call = asset_hub_westend::apis().revive_api().instantiate(
		caller_account_id,
		0,
		None,
		None,
		Code::Upload(code),
		vec![],
		None,
	);
	let dry_run = para_client.runtime_api().at_latest().await?.call(call).await?;
	let deposit = match dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	Ok((dry_run.gas_required.ref_time, dry_run.gas_required.proof_size, deposit))
}

async fn instantiate_contract(
	para_client: &OnlineClient<PolkadotConfig>,
	caller: &Keypair,
) -> Result<H160, anyhow::Error> {
	let code_path = std::env::current_dir().unwrap().join("tests/parachains/contract.polkavm");
	let code = std::fs::read(code_path)?;
	let (ref_time, proof_size, deposit) =
		instantiate_params(para_client, code.clone(), caller).await?;

	// We need a nonce before instantiating the contract
	let caller_h160 =
		<asset_hub_westend_runtime::Runtime as pallet_revive::Config>::AddressMapper::to_address(
			&caller.public_key().0.into(),
		);
	let caller_revive_nonce = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(asset_hub_westend::apis().revive_api().nonce(caller_h160))
		.await?;
	let contract_address = pallet_revive::create1(&caller_h160, caller_revive_nonce.into());

	para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().instantiate_with_code(
				0,
				Weight { ref_time, proof_size },
				deposit,
				code,
				vec![],
				None,
			),
			caller,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	Ok(contract_address)
}

async fn call_contract(
	para_client: &OnlineClient<PolkadotConfig>,
	contract_address: H160,
	caller: &Keypair,
	keys: &[Keypair],
	nonce: u64,
) -> Result<(), anyhow::Error> {
	let mint_100 = sp_core::hex2array!(
		"a0712d680000000000000000000000000000000000000000000000000000000000000064"
	)
	.to_vec();
	let (ref_time, proof_size, deposit) =
		call_params(para_client, contract_address, mint_100.clone(), caller).await?;
	let txs = keys
		.iter()
		.map(|key| {
			para_client.tx().create_signed_offline(
				&asset_hub_westend::tx().revive().call(
					contract_address,
					0,
					Weight { ref_time, proof_size },
					deposit,
					mint_100.clone(),
				),
				key,
				PolkadotExtrinsicParamsBuilder::new().nonce(nonce).build(),
			)
		})
		.collect::<Result<Vec<_>, _>>()?;
	submit_txs(txs).await?;

	Ok(())
}

async fn submit_txs(
	txs: Vec<SubmittableExtrinsic<PolkadotConfig, OnlineClient<PolkadotConfig>>>,
) -> Result<(), anyhow::Error> {
	let futs = txs.iter().map(|tx| tx.submit_and_watch()).collect::<FuturesUnordered<_>>();
	let res = futs.collect::<Vec<_>>().await;
	let res: Result<Vec<_>, _> = res.into_iter().collect();
	let res = res.expect("All the transactions submitted successfully");
	let mut statuses = futures::stream::select_all(res);
	while let Some(a) = statuses.next().await {
		match a {
			Ok(st) => match st {
				subxt::tx::TxStatus::Validated => log::trace!("VALIDATED"),
				subxt::tx::TxStatus::Broadcasted { num_peers } =>
					log::trace!("BROADCASTED TO {num_peers}"),
				subxt::tx::TxStatus::NoLongerInBestBlock => log::warn!("NO LONGER IN BEST BLOCK"),
				subxt::tx::TxStatus::InBestBlock(_) => log::trace!("IN BEST BLOCK"),
				subxt::tx::TxStatus::InFinalizedBlock(_) => log::trace!("IN FINALIZED BLOCK"),
				subxt::tx::TxStatus::Error { message } => log::warn!("ERROR: {message}"),
				subxt::tx::TxStatus::Invalid { message } => log::trace!("INVALID: {message}"),
				subxt::tx::TxStatus::Dropped { message } => log::trace!("DROPPED: {message}"),
			},
			Err(e) => {
				println!("Error status {:?}", e);
			},
		}
	}
	Ok(())
}

async fn find_shared_cache_hit_rate(logs: String) -> Result<u32, anyhow::Error> {
	let logs = logs.lines().collect::<Vec<_>>();
	let restart_index = logs.len() -
		logs.iter()
			.rev()
			.position(|l| l.contains("Local node identity is:"))
			.ok_or_else(|| anyhow!("Log not found"))?;
	// Extract score for shared cache from logs that end like this:
	// "[Parachain] ... trie cache dropped: ... shared hit rate = 100% [3/3]"
	let hit_rates_after_restart = logs[(restart_index)..]
		.iter()
		.filter(|l| l.contains("[Parachain]") && l.contains("shared hit rate ="))
		.map(|l| {
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
	for rate in hit_rates_after_restart {
		// We consider only the first 100 keys because the cache warms up on its own afterward
		if total > 100 {
			break;
		}
		hit += rate.0;
		total += rate.1;
	}
	let hit_rate = hit as f64 / total as f64 * 100.0;

	Ok(hit_rate as u32)
}
