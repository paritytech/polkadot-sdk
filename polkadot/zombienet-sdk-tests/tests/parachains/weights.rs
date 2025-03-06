// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use crate::helpers::asset_hub_westend::{
	self,
	runtime_types::{
		pallet_revive::primitives::{Code, StorageDeposit},
		sp_weights::weight_v2::Weight,
	},
};
use pallet_revive::AddressMapper;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;
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
					n.with_name("collator").validator(true).with_args(vec![
						("--force-authoring").into(),
						("-ltxpool=trace").into(),
						("--pool-type=fork-aware").into(),
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

	let validator0 = network.get_node("validator-0")?;
	let validator1 = network.get_node("validator-1")?;
	let collator = network.get_node("collator")?;

	let _relay_client: OnlineClient<PolkadotConfig> = validator0.wait_client().await?;
	let para_client: OnlineClient<PolkadotConfig> = collator.wait_client().await?;

	validator0.assert("node_roles", 4.0).await?;
	validator1.assert("node_roles", 4.0).await?;
	collator.assert("node_roles", 4.0).await?;

	let alice_signer = dev::alice();
	let alice_public = alice_signer.public_key();
	let alice_account_id = alice_public.0.into();
	let alice_public_bytes: &[u8] = alice_public.as_ref();
	let alice_h160 =
		<asset_hub_westend_runtime::Runtime as pallet_revive::Config>::AddressMapper::to_address(
			&alice_account_id,
		);
	println!("alice_h160: {:?}", alice_h160);

	para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().map_account(),
			&alice_signer,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	let code_path = std::env::current_dir().unwrap().join("tests/parachains/contract.polkavm");
	let code = std::fs::read(code_path)?;

	let alice_public = alice_signer.public_key();
	let contract_dry_run_call = asset_hub_westend::apis().revive_api().instantiate(
		alice_public.into(),
		0,
		None,
		None,
		Code::Upload(code.clone()),
		b"0x".to_vec(),
		None,
	);
	let contract_dry_run =
		para_client.runtime_api().at_latest().await?.call(contract_dry_run_call).await?;
	let storage_deposit = match contract_dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	let contract = para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().instantiate_with_code(
				0,
				Weight {
					ref_time: contract_dry_run.gas_required.ref_time,
					proof_size: contract_dry_run.gas_required.proof_size,
				},
				storage_deposit,
				code,
				b"0x".to_vec(),
				None,
			),
			&alice_signer,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	assert!(contract.find::<asset_hub_westend::system::events::NewAccount>().count() > 0);

	let alice_public = alice_signer.public_key();
	let contract_address = pallet_revive::create1(&alice_h160, 1);
	let contract_call_dry_run_call = asset_hub_westend::apis().revive_api().call(
		alice_public.into(),
		contract_address,
		0,
		None,
		None,
		b"0xa0712d680000000000000000000000000000000000000000000000000000000000000001".to_vec(),
	);
	let contract_call_dry_run = para_client
		.runtime_api()
		.at_latest()
		.await?
		.call(contract_call_dry_run_call)
		.await?;
	let storage_deposit = match contract_call_dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};
	let contract_call = para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().call(
				contract_address,
				0,
				Weight {
					ref_time: contract_call_dry_run.gas_required.ref_time,
					proof_size: contract_call_dry_run.gas_required.proof_size,
				},
				storage_deposit,
				b"0xa0712d680000000000000000000000000000000000000000000000000000000000000001"
					.to_vec(),
			),
			&alice_signer,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	// Wait to interact with PolkadotJS
	tokio::time::sleep(std::time::Duration::from_secs(600)).await;

	assert!(
		contract_call
			.find::<asset_hub_westend::revive::events::ContractEmitted>()
			.count() > 0
	);

	Ok(())
}
