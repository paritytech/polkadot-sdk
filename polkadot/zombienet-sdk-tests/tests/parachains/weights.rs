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
use std::str::FromStr;
use subxt::{tx::SubmittableExtrinsic, OnlineClient, PolkadotConfig};
use subxt_signer::{
	sr25519::{dev, Keypair},
	SecretUri,
};
use zombienet_sdk::NetworkConfigBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn weights_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

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
						.with_args(vec![("-lerror,runtime=trace").into()])
				})
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let validator0 = network.get_node("validator-0")?;
	let validator1 = network.get_node("validator-1")?;
	let collator = network.get_node("collator")?;

	let _relay_client: OnlineClient<PolkadotConfig> = validator0.wait_client().await?;
	let para_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;

	validator0.assert("node_roles", 4.0).await?;
	validator1.assert("node_roles", 4.0).await?;
	collator.assert("node_roles", 4.0).await?;

	log::info!("Network is ready");

	let alice = dev::alice();
	let alice_account_id = alice.public_key().to_account_id();
	let alice_h160 =
		<asset_hub_westend_runtime::Runtime as pallet_revive::Config>::AddressMapper::to_address(
			&alice.public_key().0.into(),
		);

	para_client
		.tx()
		.sign_and_submit_then_watch_default(&asset_hub_westend::tx().revive().map_account(), &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	let code_path = std::env::current_dir().unwrap().join("tests/parachains/contract.polkavm");
	let code = std::fs::read(code_path)?;
	let contract_data = sp_core::hex2array!(
		"a0712d680000000000000000000000000000000000000000000000000000000000000064"
	)
	.to_vec();

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
			&alice,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	let contract_dry_run = para_client
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
		ref_time: contract_dry_run.gas_required.ref_time,
		proof_size: contract_dry_run.gas_required.proof_size,
	};
	let deposit = match contract_dry_run.storage_deposit {
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
			&alice,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	let keys = (0..10)
		.map(|i| {
			let uri = SecretUri::from_str(&format!("//key{}", i)).unwrap();
			Keypair::from_uri(&uri).unwrap()
		})
		.collect::<Vec<_>>();

	let alice_nonce = para_client.tx().account_nonce(&alice_account_id).await?;
	let transfer_txs = keys
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
				&alice,
				params,
			)
		})
		.collect::<Result<Vec<_>, _>>()?;
	submit_txs(transfer_txs).await?;

	log::info!("Accounts ready");

	let mapping_txs = keys
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
	submit_txs(mapping_txs).await?;

	log::info!("Accounts mapped");

	let contract_call_dry_run = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(asset_hub_westend::apis().revive_api().call(
			alice_account_id.clone(),
			contract_address,
			0,
			None,
			None,
			contract_data.clone(),
		))
		.await?;
	let storage_deposit = match contract_call_dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	let call_txs = keys
		.iter()
		.map(|key| {
			let weight = Weight {
				ref_time: contract_call_dry_run.gas_required.ref_time,
				proof_size: contract_call_dry_run.gas_required.proof_size,
			};
			let params =
				subxt::config::polkadot::PolkadotExtrinsicParamsBuilder::new().nonce(1).build();
			para_client.tx().create_signed_offline(
				&asset_hub_westend::tx().revive().call(
					contract_address,
					0,
					weight,
					storage_deposit,
					contract_data.clone(),
				),
				key,
				params,
			)
		})
		.collect::<Result<Vec<_>, _>>()?;
	submit_txs(call_txs).await?;

	log::info!("Test finished, sleeping for 6000 seconds to allow for manual inspection");

	// Wait to interact with PolkadotJS
	tokio::time::sleep(std::time::Duration::from_secs(6000)).await;

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
