// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use anyhow::anyhow;
use sp_core::{Bytes, Encode};
use zombienet_sdk::{subxt::ext::subxt_rpcs::rpc_params, NetworkConfigBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn statement_store() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				// Have to set a `with_node` outside of the loop below, so that `r` has the right
				// type.
				.with_node(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(2400)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("people-westend-local")
				.with_default_args(vec![
					"--force-authoring".into(),
					"-lparachain=debug".into(),
					"--enable-statement-store".into(),
				])
				.with_collator(|n| n.with_name("charlie"))
				.with_collator(|n| n.with_name("dave"))
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

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	let charlie = network.get_node("charlie")?;
	let dave = network.get_node("dave")?;

	let charlie_rpc = charlie.rpc().await?;
	let dave_rpc = dave.rpc().await?;

	// Create the statement "1,2,3" signed by dave.
	let mut statement = sp_statement_store::Statement::new();
	statement.set_plain_data(vec![1, 2, 3]);
	let dave = sp_keyring::Sr25519Keyring::Dave;
	statement.sign_sr25519_private(&dave.pair());
	let statement: Bytes = statement.encode().into();

	// Submit the statement to charlie.
	let _: () = charlie_rpc.request("statement_submit", rpc_params![statement.clone()]).await?;

	// Ensure that charlie stored the statement.
	let charlie_dump: Vec<Bytes> = charlie_rpc.request("statement_dump", rpc_params![]).await?;
	if charlie_dump != vec![statement.clone()] {
		return Err(anyhow!("Charlie did not store the statement"));
	}

	// Query dave until it receives the statement, stop if 20 seconds passed.
	let query_start_time = std::time::SystemTime::now();
	let stop_after_secs = 20;
	loop {
		let dave_dump: Vec<Bytes> = dave_rpc.request("statement_dump", rpc_params![]).await?;
		if !dave_dump.is_empty() {
			if dave_dump != vec![statement.clone()] {
				return Err(anyhow!("Dave statement store is not the expected one"));
			}
			break;
		}

		let elapsed =
			query_start_time.elapsed().map_err(|_| anyhow!("Failed to get elapsed time"))?;
		if elapsed.as_secs() > stop_after_secs {
			return Err(anyhow!("Dave did not receive the statement in time"));
		}

		tokio::time::sleep(core::time::Duration::from_secs(1)).await;
	}

	Ok(())
}
