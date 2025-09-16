// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

use anyhow::anyhow;
use sp_core::{blake2_256, sr25519, Bytes, Decode, Encode, Pair};
use sp_keyring::Sr25519Keyring;
use sp_statement_store::{Statement, Topic};
use std::collections::HashMap;
use zombienet_sdk::{subxt::ext::subxt_rpcs::rpc_params, NetworkConfigBuilder};

const GROUP_SIZE: usize = 6;

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
				.with_chain_spec_path("tests/zombie_ci/people-rococo-spec.json")
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

	let mut participants: Vec<_> = [
		Sr25519Keyring::Alice,
		Sr25519Keyring::Bob,
		Sr25519Keyring::Charlie,
		Sr25519Keyring::Dave,
		Sr25519Keyring::Eve,
		Sr25519Keyring::Ferdie,
	]
	.into_iter()
	.enumerate()
	.map(|(idx, keyring)| Participant::new(keyring, idx))
	.collect();

	let mut statements = Vec::new();
	for participant in &participants {
		let statement = participant.public_key_statement();
		let statement_bytes: Bytes = statement.encode().into();
		statements.push(statement_bytes.clone());

		let _: () = collator_rpc.request("statement_submit", rpc_params![statement_bytes]).await?;
	}

	for participant in &mut participants {
		println!("Participant {:?} polling for statements from others...", participant.keyring);

		let topic = vec![topic_public_key()];

		let statements: Vec<Bytes> = collator_rpc
			.request("statement_broadcastsStatement", rpc_params![topic])
			.await?;

		for statement_bytes in &statements {
			if let Ok(statement) = sp_statement_store::Statement::decode(&mut &statement_bytes[..])
			{
				if let Some(topic_1) = statement.topic(1) {
					let other_idx = usize::from_le_bytes({
						let mut bytes = [0u8; 8];
						bytes.copy_from_slice(&topic_1[..8]);
						bytes
					});

					if let Some(data) = statement.data() {
						let slice = data.as_slice();
						if slice.len() == 32 {
							let mut array = [0u8; 32];
							array.copy_from_slice(slice);
							let session_key = sr25519::Public::from_raw(array);

							participant.add_group_member(other_idx, session_key);

							println!(
								"Participant {:?} found session key {:?} for idx {}",
								participant.keyring, session_key, other_idx
							);
						}
					}
				}
			}
		}

		println!("Participant {:?} group members: {:?}", participant.keyring, participant.group);
	}

	// Generate and exchange symmetric keys
	for participant in &mut participants {
		participant.generate_symmetric_keys();

		// Send symmetric keys to group members with higher idx
		for &receiver_idx in participant.group.keys() {
			if receiver_idx > participant.idx {
				if let Some(statement) = participant.symmetric_key_statement(receiver_idx) {
					println!(
						"Sending {:?} symmetric key to idx {} {:?}",
						participant.keyring,
						receiver_idx,
						statement.channel()
					);
					let statement_bytes: Bytes = statement.encode().into();
					let _: () = collator_rpc
						.request("statement_submit", rpc_params![statement_bytes])
						.await?;
				}
			}
		}
	}

	// Poll for symmetric keys from other participants
	for participant in &mut participants {
		println!("Participant {:?} polling for symmetric keys...", participant.keyring);

		// Check for symmetric keys from each group member with lower idx
		for (&sender_idx, sender_session_key) in participant.group.iter() {
			if sender_idx < participant.idx {
				let topic =
					vec![topic_for_pair(sender_session_key, &participant.session_key.public())];

				let statements: Vec<Bytes> = collator_rpc
					.request("statement_broadcastsStatement", rpc_params![topic])
					.await?;

				for statement_bytes in &statements {
					if let Ok(statement) =
						sp_statement_store::Statement::decode(&mut &statement_bytes[..])
					{
						if let Some(data) = statement.data() {
							if data.len() == 32 {
								let mut symmetric_key = [0u8; 32];
								symmetric_key.copy_from_slice(data);
								participant.symmetric_keys.insert(sender_idx, symmetric_key);

								println!(
									"Participant {:?} received symmetric key from idx {}",
									participant.keyring, sender_idx
								);
							}
						}
					}
				}
			}
		}

		println!(
			"Participant {:?} symmetric keys: {} keys",
			participant.keyring,
			participant.symmetric_keys.len()
		);
	}

	Ok(())
}

struct Participant {
	keyring: Sr25519Keyring,
	session_key: sr25519::Pair,
	idx: usize,
	group: HashMap<usize, sr25519::Public>,
	symmetric_keys: HashMap<usize, [u8; 32]>,
}

impl Participant {
	fn new(keyring: Sr25519Keyring, idx: usize) -> Self {
		let (session_key, _) = sr25519::Pair::generate();
		Self { keyring, session_key, idx, group: HashMap::new(), symmetric_keys: HashMap::new() }
	}

	fn public_key_statement(&self) -> Statement {
		let mut statement = Statement::new();
		statement.set_channel([0u8; 32]);
		statement.set_topic(0, topic_public_key());
		statement.set_topic(1, topic_idx(self.idx));
		statement.set_plain_data(self.session_key.public().to_vec());
		statement.sign_sr25519_private(&self.keyring.pair());

		statement
	}

	fn is_in_same_group(&self, other_idx: usize) -> bool {
		self.idx / GROUP_SIZE == other_idx / GROUP_SIZE
	}

	fn add_group_member(&mut self, idx: usize, session_key: sr25519::Public) {
		if self.is_in_same_group(idx) && idx != self.idx {
			self.group.insert(idx, session_key);
		}
	}

	fn generate_symmetric_keys(&mut self) {
		for &other_idx in self.group.keys() {
			if other_idx > self.idx {
				let mut key_material = Vec::new();
				key_material.extend_from_slice(&self.idx.to_le_bytes());
				key_material.extend_from_slice(&other_idx.to_le_bytes());
				let symmetric_key = blake2_256(&key_material);
				self.symmetric_keys.insert(other_idx, symmetric_key);
			}
		}
	}

	fn symmetric_key_statement(&self, receiver_idx: usize) -> Option<Statement> {
		if let (Some(symmetric_key), Some(receiver_session_key)) =
			(self.symmetric_keys.get(&receiver_idx), self.group.get(&receiver_idx))
		{
			let mut statement = Statement::new();

			let topic = topic_for_pair(&self.session_key.public(), receiver_session_key);
			let channel = channel_for_pair(&self.session_key.public(), receiver_session_key, 0);

			statement.set_channel(channel);
			statement.set_topic(0, topic);
			statement.set_plain_data(symmetric_key.to_vec());
			statement.sign_sr25519_private(&self.keyring.pair());

			Some(statement)
		} else {
			None
		}
	}
}

fn topic_public_key() -> Topic {
	let mut topic = [0u8; 32];
	let source = b"public key";
	let len = source.len().min(32);
	topic[..len].copy_from_slice(&source[..len]);
	topic
}

fn topic_idx(idx: usize) -> Topic {
	let mut topic = [0u8; 32];
	topic[..8].copy_from_slice(&idx.to_le_bytes());
	topic
}

fn topic_for_pair(sender: &sr25519::Public, receiver: &sr25519::Public) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	blake2_256(&data)
}

fn channel_for_pair(
	sender: &sr25519::Public,
	receiver: &sr25519::Public,
	message_counter: u64,
) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	data.extend_from_slice(&message_counter.to_le_bytes());
	blake2_256(&data)
}
