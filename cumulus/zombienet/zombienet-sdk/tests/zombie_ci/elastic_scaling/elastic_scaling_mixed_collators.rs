// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that parachain that use a mix of collators can produce blocks but with an expected
// degradation.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_relay_parent_offset, create_assign_core_call};
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	LocalFileSystem, NetworkConfigBuilder, Orchestrator,
};

use zombienet_configuration::types::AssetLocation;
use zombienet_orchestrator::ScopedFilesystem;

#[tokio::test(flavor = "multi_thread")]
async fn elastic_scaling_mixed_collators_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// Num cores is 4, because 2 extra will be added automatically when registering the paras.
								"num_cores": 4,
								// "lookahead": 8,
								"max_validators_per_core": 1
							}
						}
					}
				}))
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("test-parachain-2506")
				.with_default_image(images.cumulus.as_str())
				.with_chain("relay-parent-offset")
				.with_default_args(vec![
					"--authoring=slot-based".into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2506").with_command("test-parachain-2506"))
				.with_collator(|n| n.with_name("collator-2509").with_command("test-parachain-2509"))
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	// Build chain-spec and modify the content of the raw version

	let filesystem = LocalFileSystem;
	let provider = NativeProvider::new(filesystem.clone());
	let ns = provider.create_namespace().await?;

	let base_dir = ns.base_dir().to_string_lossy();
	let scoped_fs = ScopedFilesystem::new(&filesystem, &base_dir);

	let mut network_spec = zombienet_orchestrator::NetworkSpec::from_config(&config).await?;
	let para = network_spec.parachains_iter_mut().next().unwrap();
	let chain_spec = para.chain_spec_mut().unwrap();
	chain_spec.build(&ns, &scoped_fs).await?;
	chain_spec.build_raw(&ns, &scoped_fs).await?;

	let raw_path = chain_spec.raw_path().unwrap();
	let file_full_path = format!("{base_dir}/{}", raw_path.to_string_lossy());
	let content = tokio::fs::read_to_string(&file_full_path).await?;
	let mut content_json: serde_json::Value = serde_json::from_str(&content)?;
	content_json["para_id"] = json!(2000);
	let _ = tokio::fs::write(&file_full_path, serde_json::to_string(&content_json)?).await?;
	chain_spec.set_asset_location(AssetLocation::FilePath(PathBuf::from_str(&file_full_path)?));

	let orchestrator = Orchestrator::new(filesystem, provider.clone());
	let network = orchestrator.spawn_from_spec(network_spec).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node_rp_offset = network.get_node("collator-2509")?;

	let para_client = para_node_rp_offset.wait_client().await?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let alice = dev::alice();

	let assign_cores_call = create_assign_core_call(&[(0, 2400), (1, 2400)]);
	// Assign two extra cores to each parachain.
	relay_client
		.tx()
		.sign_and_submit_then_watch_default(&assign_cores_call, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("2 more cores assigned to the parachain");

	tokio::time::sleep(std::time::Duration::from_secs(60 * 600)).await;
	assert_relay_parent_offset(&relay_client, &para_client, 2, 30).await?;

	log::info!("Test finished successfully");

	Ok(())
}
