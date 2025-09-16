// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use anyhow::anyhow;
use sp_core::{Bytes, Decode, Encode};
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
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_node(|node| node.with_name("validator-0"))
				.with_node(|node| node.with_name("validator-1"))
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
				.with_collator(|n| n.with_name("alice"))
				.with_collator(|n| n.with_name("bob"))
				.with_collator(|n| n.with_name("charlie"))
				.with_collator(|n| n.with_name("dave"))
				.with_collator(|n| n.with_name("eve"))
				.with_collator(|n| n.with_name("ferdie"))
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

	let charlie_node = network.get_node("charlie")?;
	let charlie_rpc = charlie_node.rpc().await?;

	let alice = sp_keyring::Sr25519Keyring::Alice;
	let bob = sp_keyring::Sr25519Keyring::Bob;
	let charlie = sp_keyring::Sr25519Keyring::Charlie;
	let dave = sp_keyring::Sr25519Keyring::Dave;
	let eve = sp_keyring::Sr25519Keyring::Eve;
	let ferdie = sp_keyring::Sr25519Keyring::Ferdie;

	// Create the statement "1,2,3" signed by dave.
	let mut statement = sp_statement_store::Statement::new();
	statement.set_channel([0u8; 32]);
	statement.set_plain_data(vec![1, 2, 3]);

	statement.sign_sr25519_private(&dave.pair());
	let statement: Bytes = statement.encode().into();

	// Submit the statement to charlie.
	let _: () = charlie_rpc.request("statement_submit", rpc_params![statement.clone()]).await?;

	// Ensure that charlie stored the statement.
	let charlie_dump: Vec<Bytes> = charlie_rpc.request("statement_dump", rpc_params![]).await?;
	if charlie_dump != vec![statement.clone()] {
		return Err(anyhow!("charlie did not store the statement"));
	}

	let statement_bytes = charlie_dump.first().unwrap();
	if let Ok(statement) = sp_statement_store::Statement::decode(&mut &statement_bytes[..]) {
		println!("{:?}", statement);
	}

	Ok(())
}
