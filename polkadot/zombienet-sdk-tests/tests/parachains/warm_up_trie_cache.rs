// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// TODO: Use precompiled metadata when v15 has become by default
// `#[subxt::subxt(runtime_metadata_path = "metadata-files/asset-hub-westend-local.scale")]`
// Don't forget to remove `subxt-macro` from all Cargo.toml files
#[subxt::subxt(runtime_metadata_insecure_url = "wss://westend-asset-hub-rpc.polkadot.io:443")]
mod ahw {}

use ahw::runtime_types::{
	pallet_revive::primitives::{Code, StorageDeposit},
	sp_weights::weight_v2::Weight,
};
use anyhow::anyhow;
use asset_hub_westend_runtime::Runtime as AHWRuntime;
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

/// Verifies the effectiveness of the trie cache warming mechanism
///
/// Passes if the cache hit rate is above 85%, indicating that the trie cache
/// was successfully warmed up and is effectively reducing storage access after node restart.
///
/// 1. Setting up a network with a collator node, immitating Asset Hub with smart contracts
/// 2. Creating multiple accounts and a smart contract
/// 3. Making contract calls to populate storage
/// 4. Restarting the collator node
/// 5. Making the same contract calls again
/// 6. Measuring the cache hit rate after restart
#[tokio::test(flavor = "multi_thread")]
async fn warm_up_trie_cache_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let hit_rate = run(100).await?;
	assert!(hit_rate > 85);

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
					std::env::var("CUMULUS_IMAGE")
						.unwrap_or("docker.io/paritypr/polkadot-parachain-debug:latest".to_string())
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

fn tx_params<T: subxt::Config>(
	nonce: u64,
) -> <subxt::config::DefaultExtrinsicParams<T> as subxt::config::ExtrinsicParams<T>>::Params {
	PolkadotExtrinsicParamsBuilder::<T>::new().nonce(nonce).build()
}

async fn setup_accounts(
	client: &OnlineClient<PolkadotConfig>,
	caller: &Keypair,
	keys: &[Keypair],
	nonce: u64,
) -> Result<(), anyhow::Error> {
	let caller_account_id = caller.public_key().to_account_id();
	let caller_nonce = client.tx().account_nonce(&caller_account_id).await?;

	let transfers = keys
		.iter()
		.enumerate()
		.map(|(i, key)| {
			let key = key.public_key().into();
			let call = &ahw::tx().balances().transfer_keep_alive(key, 1000000000000);
			let params = tx_params(caller_nonce + i as u64);
			client.tx().create_signed_offline(call, caller, params)
		})
		.collect::<Result<Vec<_>, _>>()?;
	submit_txs(transfers).await?;

	let map_call = &ahw::tx().revive().map_account();
	let mappings = keys
		.iter()
		.map(|key| (key, nonce))
		.chain(std::iter::once((caller, caller_nonce + keys.len() as u64)))
		.map(|(k, n)| client.tx().create_signed_offline(map_call, k, tx_params(n)))
		.collect::<Result<Vec<_>, _>>()?;
	submit_txs(mappings).await?;

	Ok(())
}

async fn call_params(
	client: &OnlineClient<PolkadotConfig>,
	contract: H160,
	payload: Vec<u8>,
	caller: &Keypair,
) -> Result<(u64, u64, u128), anyhow::Error> {
	let account_id = caller.public_key().to_account_id();
	let call = ahw::apis().revive_api().call(account_id, contract, 0, None, None, payload);
	let dry_run = client.runtime_api().at_latest().await?.call(call).await?;
	let deposit = match dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	Ok((dry_run.gas_required.ref_time, dry_run.gas_required.proof_size, deposit))
}

async fn instantiate_params(
	client: &OnlineClient<PolkadotConfig>,
	code: Vec<u8>,
	caller: &Keypair,
) -> Result<(u64, u64, u128), anyhow::Error> {
	let account_id = caller.public_key().to_account_id();
	let code = Code::Upload(code);
	let call = ahw::apis()
		.revive_api()
		.instantiate(account_id, 0, None, None, code, vec![], None);
	let dry_run = client.runtime_api().at_latest().await?.call(call).await?;
	let deposit = match dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	Ok((dry_run.gas_required.ref_time, dry_run.gas_required.proof_size, deposit))
}

async fn instantiate_contract(
	client: &OnlineClient<PolkadotConfig>,
	caller: &Keypair,
) -> Result<H160, anyhow::Error> {
	let code_path = std::env::current_dir().unwrap().join("tests/parachains/contract.polkavm");
	let code = std::fs::read(code_path)?;
	let (ref_time, proof_size, deposit) = instantiate_params(client, code.clone(), caller).await?;

	// We need a nonce before instantiating the contract
	let account_id = caller.public_key().0.into();
	let caller_h160 = <AHWRuntime as pallet_revive::Config>::AddressMapper::to_address(&account_id);
	let caller_revive_nonce = client
		.runtime_api()
		.at_latest()
		.await?
		.call(ahw::apis().revive_api().nonce(caller_h160))
		.await?;
	let contract_address = pallet_revive::create1(&caller_h160, caller_revive_nonce.into());
	let weight = Weight { ref_time, proof_size };
	let call = &ahw::tx().revive().instantiate_with_code(0, weight, deposit, code, vec![], None);
	client
		.tx()
		.sign_and_submit_then_watch_default(call, caller)
		.await?
		.wait_for_finalized_success()
		.await?;

	Ok(contract_address)
}

async fn call_contract(
	client: &OnlineClient<PolkadotConfig>,
	contract: H160,
	caller: &Keypair,
	keys: &[Keypair],
	nonce: u64,
) -> Result<(), anyhow::Error> {
	let mint_100 = sp_core::hex2array!(
		"a0712d680000000000000000000000000000000000000000000000000000000000000064"
	)
	.to_vec();
	let (ref_time, proof_size, deposit) =
		call_params(client, contract, mint_100.clone(), caller).await?;
	let weight = Weight { ref_time, proof_size };
	let call = &ahw::tx().revive().call(contract, 0, weight, deposit, mint_100.clone());
	let txs = keys
		.iter()
		.map(|key| client.tx().create_signed_offline(call, key, tx_params(nonce)))
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
