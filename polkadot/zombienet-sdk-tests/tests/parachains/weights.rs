// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#[zombienet_sdk::subxt::subxt(
	runtime_metadata_path = "metadata-files/asset-hub-westend-local.scale"
)]
mod ahw {}

use ahw::runtime_types::{
	pallet_revive::primitives::{Code, StorageDeposit},
	sp_weights::weight_v2::Weight,
};
use anyhow::anyhow;
use asset_hub_westend_runtime::Runtime as AHWRuntime;
use futures::{stream::FuturesUnordered, StreamExt};
use pallet_revive::AddressMapper;
use sp_core::{H160, H256};
use std::str::FromStr;
use zombienet_sdk::{
	subxt::{
		self, config::polkadot::PolkadotExtrinsicParamsBuilder, tx::SubmittableExtrinsic,
		OnlineClient, PolkadotConfig,
	},
	subxt_signer::{
		sr25519::{dev, Keypair},
		SecretUri,
	},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

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
	for _ in 0..(KEYS_COUNT / CALL_CHUNK_SIZE + 1) {
		let call_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
		call_clients.push(call_client);
	}
	log::info!("Network is ready");

	let alice = dev::alice();
	let keys = create_keys(KEYS_COUNT);
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

	let mint_100 = sp_core::hex2array!(
		"a0712d680000000000000000000000000000000000000000000000000000000000000064"
	)
	.to_vec();
	call_contract(&para_client, call_clients, contract_address, &alice, &keys, nonce(), mint_100)
		.await?;

	let mut call_clients = vec![];
	for _ in 0..(KEYS_COUNT / CALL_CHUNK_SIZE + 1) {
		let call_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;
		call_clients.push(call_client);
	}
	let transfer_50_to_alice = sp_core::hex2array!(
		"a9059cbb0000000000000000000000009621dde636de098b43efb0fa9b61facfe328f99d0000000000000000000000000000000000000000000000000000000000000032"
	)
	.to_vec();
	call_contract(
		&para_client,
		call_clients,
		contract_address,
		&alice,
		&keys,
		nonce(),
		transfer_50_to_alice,
	)
	.await?;

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
					n.with_name("collator")
						.validator(true)
						.with_args(vec![("--warm-up-trie-cache").into(), ("-linfo").into()])
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
	let mut caller_nonce = client.tx().account_nonce(&caller_account_id).await?;
	for chunk in keys.chunks(CHUNK_SIZE) {
		let transfers = chunk
			.iter()
			.enumerate()
			.map(|(i, key)| {
				let key = key.public_key().into();
				let call = &ahw::tx().balances().transfer_keep_alive(key, 1000000000000);
				let params = tx_params(caller_nonce);
				caller_nonce += 1;
				client.tx().create_signed_offline(call, caller, params)
			})
			.collect::<Result<Vec<_>, _>>()?;
		submit_txs(transfers).await?;
	}

	let map_call = &ahw::tx().revive().map_account();
	let mut is_caller_mapped = false;
	for chunk in keys.chunks(CHUNK_SIZE) {
		let mappings = chunk
			.iter()
			.map(|key| (key, nonce))
			.chain(if is_caller_mapped {
				None
			} else {
				is_caller_mapped = true;
				Some((caller, caller_nonce))
			})
			.map(|(k, n)| client.tx().create_signed_offline(map_call, k, tx_params(n)))
			.collect::<Result<Vec<_>, _>>()?;
		submit_txs(mappings).await?;
	}

	Ok(())
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
	log::info!("H160 Account: {:?}", caller_h160);
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

async fn call_contract(
	client: &OnlineClient<PolkadotConfig>,
	mut call_clients: Vec<OnlineClient<PolkadotConfig>>,
	contract: H160,
	caller: &Keypair,
	keys: &[Keypair],
	nonce: u64,
	payload: Vec<u8>,
) -> Result<(), anyhow::Error> {
	let (ref_time, proof_size, deposit) =
		call_params(client, contract, payload.clone(), caller).await?;
	let weight = Weight { ref_time, proof_size };
	let call = &ahw::tx().revive().call(contract, 0, weight, deposit, payload.clone());

	let mut txs = vec![];
	for chunk in keys.chunks(CALL_CHUNK_SIZE) {
		let para_client = call_clients.pop().unwrap();
		let txs_chunk = chunk
			.iter()
			.map(|key| para_client.tx().create_signed_offline(call, key, tx_params(nonce)))
			.collect::<Result<Vec<_>, _>>()?;
		txs.extend(txs_chunk);
	}
	let finalized_blocks = submit_txs(txs).await?;
	for block in finalized_blocks {
		let weight = client
			.storage()
			.at(block)
			.fetch(&ahw::storage().system().block_weight())
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
