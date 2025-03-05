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
use pallet_revive::{AddressMapper, Config};
use pallet_revive_mock_network::parachain::Runtime;
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
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
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
					n.with_name("charlie").validator(true).with_args(vec![
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

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;

	let _relay_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;
	let para_client: OnlineClient<PolkadotConfig> = charlie.wait_client().await?;

	alice.assert("node_roles", 4.0).await?;
	bob.assert("node_roles", 4.0).await?;
	charlie.assert("node_roles", 4.0).await?;

	let alice_signer = dev::alice();
	let alice_h160 =
		<Runtime as Config>::AddressMapper::to_address(&alice_signer.public_key().0.into());

	let nonce_call = asset_hub_westend::apis()
		.account_nonce_api()
		.account_nonce(alice_signer.public_key().into());
	let nonce = para_client.runtime_api().at_latest().await?.call(nonce_call).await?;

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

	let dry_run_call = asset_hub_westend::apis().revive_api().instantiate(
		alice_signer.public_key().into(),
		0,
		None,
		None,
		Code::Upload(code.clone()),
		b"0x".to_vec(),
		None,
	);
	let dry_run = para_client.runtime_api().at_latest().await?.call(dry_run_call).await?;
	let storage_deposit = match dry_run.storage_deposit {
		StorageDeposit::Charge(c) => c,
		StorageDeposit::Refund(_) => 0,
	};

	let xxx = para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().instantiate_with_code(
				0,
				Weight {
					ref_time: dry_run.gas_required.ref_time,
					proof_size: dry_run.gas_required.proof_size,
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

	let contract_address = pallet_revive::create1(&alice_h160, nonce.into());

	let xxx = para_client
		.tx()
		.sign_and_submit_then_watch_default(
			&asset_hub_westend::tx().revive().call(
				contract_address,
				0,
				Weight { ref_time: 1000000000, proof_size: 100000 },
				20032000000,
				b"0xa0712d6800000000000000000000000000000000000000000000000000000000000003e8"
					.to_vec(),
			),
			&alice_signer,
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	let events = xxx.all_events_in_block();

	// Example of finding specific events
	let contract_events = events
		.find::<asset_hub_westend::revive::events::ContractEmitted>()
		.collect::<Vec<_>>();

	Ok(())
}
