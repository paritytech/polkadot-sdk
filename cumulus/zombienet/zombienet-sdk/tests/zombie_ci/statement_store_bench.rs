// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use anyhow::anyhow;
use sp_core::{ed25519, Bytes, Decode, Encode, Pair};
use sp_keyring::Sr25519Keyring;
use sp_statement_store::{Statement, Topic};
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

	let collator_node = network.get_node("charlie")?;
	let collator_rpc = collator_node.rpc().await?;

	let participants: Vec<_> = [
		Sr25519Keyring::Alice,
		Sr25519Keyring::Bob,
		Sr25519Keyring::Charlie,
		Sr25519Keyring::Dave,
		Sr25519Keyring::Eve,
		Sr25519Keyring::Ferdie,
	]
	.into_iter()
	.map(Participant::new)
	.collect();

	let mut statements = Vec::new();
	for participant in &participants {
		let statement = participant.public_key_statement();
		let statement_bytes: Bytes = statement.encode().into();
		statements.push(statement_bytes.clone());

		// Submit each participant's statement to charlie.
		let _: () = collator_rpc.request("statement_submit", rpc_params![statement_bytes]).await?;
	}

	// Ensure that charlie stored all statements.
	let charlie_dump: Vec<Bytes> = collator_rpc.request("statement_dump", rpc_params![]).await?;
	if charlie_dump.len() != statements.len() {
		return Err(anyhow!(
			"charlie did not store all statements, expected {}, got {}",
			statements.len(),
			charlie_dump.len()
		));
	}

	for statement_bytes in &charlie_dump {
		if let Ok(statement) = sp_statement_store::Statement::decode(&mut &statement_bytes[..]) {
			println!("{:?}", statement);
		}
	}

	Ok(())
}

struct Participant {
	keyring: Sr25519Keyring,
	session_key: ed25519::Pair,
}

impl Participant {
	fn new(keyring: Sr25519Keyring) -> Self {
		let (session_key, _) = ed25519::Pair::generate();
		Self { keyring, session_key }
	}

	fn public_key_statement(&self) -> Statement {
		let mut statement = Statement::new();
		statement.set_channel([0u8; 32]);
		statement.set_topic(0, topic_public_key());
		statement.set_plain_data(self.session_key.public().to_vec());
		statement.sign_sr25519_private(&self.keyring.pair());

		statement
	}
}

fn topic_public_key() -> Topic {
	let mut topic = [0u8; 32];
	let source = b"public key";
	let len = source.len().min(32);
	topic[..len].copy_from_slice(&source[..len]);
	topic
}
