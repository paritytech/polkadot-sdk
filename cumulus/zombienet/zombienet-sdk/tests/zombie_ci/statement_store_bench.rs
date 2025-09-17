// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that people-westend enables the statement store in the node and that statements are
// propagated to peers.

// Removed AES-GCM import - using simple XOR for performance testing
use anyhow::anyhow;
use codec::{Decode, Encode};
use sp_core::{blake2_256, ed25519, sr25519, Bytes, Pair};
use sp_keyring::Sr25519Keyring;
use sp_statement_store::{Statement, Topic};
use std::collections::{HashMap, HashSet};
use zombienet_sdk::{
	subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params},
	NetworkConfigBuilder,
};

const GROUP_SIZE: usize = 6;
const MESSAGE_SIZE: usize = 5 * 1024; // 5KiB
const MESSAGE_COUNT: usize = 2;

#[tokio::test(flavor = "multi_thread")]
async fn statement_store() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let collator_rpc = spawn_network().await?;

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

	println!("=== Session keys exchange ===");

	// Send session keys
	for participant in &participants {
		let statement = participant.public_key_statement();
		let statement_bytes: Bytes = statement.encode().into();

		let _: () = collator_rpc.request("statement_submit", rpc_params![statement_bytes]).await?;
	}

	// Receive session keys
	for participant in &mut participants {
		let topic = vec![topic_public_key()];

		let statements: Vec<Bytes> = collator_rpc
			.request("statement_broadcastsStatement", rpc_params![topic])
			.await?;
		for statement_bytes in &statements {
			let statement = Statement::decode(&mut &statement_bytes[..])?;
			let topic1 = statement.topic(1).expect("Must contain idx");
			let other_idx = usize::from_le_bytes(topic1[..8].try_into()?);
			let data = statement.data().expect("Must contain session_key");
			let session_key = sr25519::Public::from_raw(data[..].try_into()?);
			participant.add_group_member(other_idx, session_key);
		}

		assert_eq!(participant.session_keys.len(), GROUP_SIZE - 1);
	}

	println!("=== Symmetric keys exchange ===");

	// Send symmetric keys
	for participant in &mut participants {
		for &receiver_idx in participant.session_keys.keys() {
			let Some(statement) = participant.symmetric_key_statement(receiver_idx) else {
				continue
			};
			let statement_bytes: Bytes = statement.encode().into();
			let _: () =
				collator_rpc.request("statement_submit", rpc_params![statement_bytes]).await?;
		}
	}

	// Receive symmetric keys from other participants
	for participant in &mut participants {
		// Check for symmetric keys from each group member with lower idx
		for (&sender_idx, sender_session_key) in participant.session_keys.iter() {
			if sender_idx < participant.idx {
				let topic1 = topic_for_pair(sender_session_key, &participant.session_key.public());
				let topics = vec![topic1];
				let statements: Vec<Bytes> = collator_rpc
					.request("statement_broadcastsStatement", rpc_params![topics])
					.await?;
				for statement_bytes in &statements {
					let statement = Statement::decode(&mut &statement_bytes[..])?;
					let data = statement.data().expect("Must contain symmetric key");
					participant
						.symmetric_keys
						.insert(sender_idx, ed25519::Public::from_raw(data.as_slice().try_into()?));
				}
			}
		}

		assert_eq!(participant.symmetric_keys.len(), GROUP_SIZE - 1);
	}

	let statements: Vec<Bytes> = collator_rpc
		.request("statement_broadcastsStatement", rpc_params![vec![blake2_256(b"request")]])
		.await?;
	assert_eq!(statements.len(), 0);

	let statements: Vec<Bytes> = collator_rpc
		.request("statement_broadcastsStatement", rpc_params![vec![blake2_256(b"response")]])
		.await?;
	assert_eq!(statements.len(), 0);

	for i in 0..MESSAGE_COUNT {
		println!("=== Req/res exchange round {} ===", i + 1);

		// Send request
		for participant in &mut participants {
			let receiver_indices = participant.session_keys.keys().cloned().collect::<Vec<_>>();
			for &receiver_idx in &receiver_indices {
				let request_statement = participant
					.create_request_statement(receiver_idx)
					.expect("Receiver must present");
				let statement_bytes: Bytes = request_statement.encode().into();
				let _: () =
					collator_rpc.request("statement_submit", rpc_params![statement_bytes]).await?;
			}
		}

		let statements: Vec<Bytes> = collator_rpc
			.request("statement_broadcastsStatement", rpc_params![vec![blake2_256(b"request")]])
			.await?;
		assert_eq!(statements.len(), GROUP_SIZE * (GROUP_SIZE - 1) * (i + 1));

		// Receive request
		for participant in &mut participants {
			let senders = participant.session_keys.clone();
			for (&sender_idx, sender_key) in &senders {
				let topic0 = blake2_256(b"request");
				let topic1 = topic_for_pair(&sender_key, &participant.session_key.public());
				let topics = vec![topic0, topic1];

				let statements: Vec<Bytes> = collator_rpc
					.request("statement_broadcastsStatement", rpc_params![topics])
					.await?;
				assert_eq!(statements.len(), (i + 1));

				for statement_bytes in &statements {
					let statement = Statement::decode(&mut &statement_bytes[..])?;
					let data = statement.data().expect("Must contain request");
					let req = StatementRequest::decode(&mut &data[..])?;

					if !participant.has_processed_request(sender_idx, req.request_id) {
						assert!(!participant.has_pending_response(sender_idx));
						participant.pending_responses.insert(sender_idx, Some(req.request_id));
					}
				}
			}
		}

		// Send response
		for participant in &mut participants {
			let receiver_indices = participant.session_keys.keys().cloned().collect::<Vec<_>>();
			for &receiver_idx in &receiver_indices {
				let req_id =
					participant.pending_responses.get_mut(&receiver_idx).unwrap().take().unwrap();

				let request_statement = participant
					.create_response_statement(req_id, receiver_idx)
					.expect("Receiver must present");
				let statement_bytes: Bytes = request_statement.encode().into();
				let _: () =
					collator_rpc.request("statement_submit", rpc_params![statement_bytes]).await?;
				participant.processed_requests.entry(receiver_idx).or_default().insert(req_id);
			}
		}

		let statements: Vec<Bytes> = collator_rpc
			.request("statement_broadcastsStatement", rpc_params![vec![blake2_256(b"response")]])
			.await?;
		assert_eq!(statements.len(), GROUP_SIZE * (GROUP_SIZE - 1) * (i + 1));

		// Receive response
		for participant in &mut participants {
			let senders = participant.session_keys.clone();
			for (&sender_idx, sender_key) in &senders {
				let topic0 = blake2_256(b"response");
				let topic1 = topic_for_pair(&sender_key, &participant.session_key.public());
				let topics = vec![topic0, topic1];

				let statements: Vec<Bytes> = collator_rpc
					.request("statement_broadcastsStatement", rpc_params![topics])
					.await?;
				assert_eq!(statements.len(), (i + 1));

				for statement_bytes in &statements {
					let statement = Statement::decode(&mut &statement_bytes[..])?;
					let data = statement.data().expect("Must contain response");
					let res = StatementResponse::decode(&mut &data[..])?;
					if !participant
						.received_responses
						.get(&sender_idx)
						.map_or(false, |ress| ress.contains(&res.request_id))
					{
						participant
							.received_responses
							.entry(sender_idx)
							.or_default()
							.insert(res.request_id);
					}
				}
			}
		}

		println!("All messages in the round processed");
	}

	for participant in &participants {
		assert_eq!(
			participant.received_responses.values().map(|set| set.len()).sum::<usize>(),
			(GROUP_SIZE - 1) * MESSAGE_COUNT
		)
	}

	Ok(())
}

async fn spawn_network() -> Result<RpcClient, anyhow::Error> {
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
					"-lstatement-store=trace".into(),
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

	Ok(collator_rpc)
}

#[derive(Encode, Decode, Clone)]
struct StatementRequest {
	request_id: u64,
	data: Vec<u8>,
}

#[derive(Encode, Decode, Clone)]
struct StatementResponse {
	request_id: u64,
	response_code: u8,
}

struct Participant {
	keyring: Sr25519Keyring,
	session_key: sr25519::Pair,
	idx: usize,
	session_keys: HashMap<usize, sr25519::Public>,
	symmetric_keys: HashMap<usize, ed25519::Public>,
	request_counter: u64,
	pending_responses: HashMap<usize, Option<u64>>,
	processed_requests: HashMap<usize, HashSet<u64>>,
	received_responses: HashMap<usize, HashSet<u64>>,
}

impl Participant {
	fn new(keyring: Sr25519Keyring, idx: usize) -> Self {
		let (session_key, _) = sr25519::Pair::generate();
		let mut participant = Self {
			keyring,
			session_key,
			idx,
			session_keys: HashMap::new(),
			symmetric_keys: HashMap::new(),
			request_counter: 0,
			pending_responses: HashMap::new(),
			processed_requests: HashMap::new(),
			received_responses: HashMap::new(),
		};

		participant.generate_symmetric_keys();
		participant
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
			self.session_keys.insert(idx, session_key);
		}
	}

	fn has_processed_request(&self, sender_idx: usize, request_id: u64) -> bool {
		self.processed_requests
			.get(&sender_idx)
			.map_or(false, |reqs| reqs.contains(&request_id))
	}

	fn has_pending_response(&self, sender_idx: usize) -> bool {
		self.pending_responses.get(&sender_idx).map_or(false, |res| res.is_some())
	}

	fn generate_symmetric_keys(&mut self) {
		let group_start = (self.idx / GROUP_SIZE) * GROUP_SIZE;
		let group_end = group_start + GROUP_SIZE;

		for other_idx in group_start..group_end {
			if other_idx > self.idx {
				let (pair, _) = ed25519::Pair::generate();
				self.symmetric_keys.insert(other_idx, pair.public());
			}
		}
	}

	fn symmetric_key_statement(&self, receiver_idx: usize) -> Option<Statement> {
		let (Some(symmetric_key), Some(receiver_session_key)) =
			(self.symmetric_keys.get(&receiver_idx), self.session_keys.get(&receiver_idx))
		else {
			return None
		};

		let mut statement = Statement::new();

		let topic = topic_for_pair(&self.session_key.public(), receiver_session_key);
		let channel = channel_for_pair(&self.session_key.public(), receiver_session_key, 0);

		statement.set_channel(channel);
		statement.set_topic(0, topic);
		statement.set_plain_data(symmetric_key.to_vec());
		statement.sign_sr25519_private(&self.keyring.pair());

		Some(statement)
	}

	fn create_request_statement(&mut self, receiver_idx: usize) -> Option<Statement> {
		let (Some(_symmetric_key), Some(receiver_session_key)) =
			(self.symmetric_keys.get(&receiver_idx), self.session_keys.get(&receiver_idx))
		else {
			return None
		};

		self.request_counter += 1;
		let request_id = self.request_counter;

		// Create 5KiB payload
		let mut data = vec![0u8; MESSAGE_SIZE];
		for (i, byte) in data.iter_mut().enumerate() {
			*byte = (i % 256) as u8; // Simple pattern for testing
		}

		let request = StatementRequest { request_id, data };
		let request_data = request.encode();
		let mut statement = Statement::new();

		let topic0 = blake2_256(b"request");
		let topic1 = topic_for_pair(&self.session_key.public(), receiver_session_key);
		let channel =
			channel_for_request(&self.session_key.public(), receiver_session_key, request_id);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_plain_data(request_data);
		statement.sign_sr25519_private(&self.keyring.pair());

		println!(
			"Participant {:?} created request {} to idx {}",
			self.keyring, request_id, receiver_idx
		);

		Some(statement)
	}

	fn create_response_statement(
		&mut self,
		request_id: u64,
		receiver_idx: usize,
	) -> Option<Statement> {
		let (Some(_symmetric_key), Some(receiver_session_key)) =
			(self.symmetric_keys.get(&receiver_idx), self.session_keys.get(&receiver_idx))
		else {
			return None
		};

		let response = StatementResponse { request_id, response_code: 0 };
		let response_data = response.encode();

		let mut statement = Statement::new();

		let topic0 = blake2_256(b"response");
		let topic1 = topic_for_pair(&self.session_key.public(), receiver_session_key);
		let channel =
			channel_for_response(&self.session_key.public(), receiver_session_key, request_id);

		statement.set_topic(0, topic0);
		statement.set_topic(1, topic1);
		statement.set_channel(channel);
		statement.set_plain_data(response_data);
		statement.sign_sr25519_private(&self.keyring.pair());

		println!(
			"Participant {:?} created response for request {} to idx {}",
			self.keyring, request_id, receiver_idx
		);

		Some(statement)
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
	message_counter: usize,
) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	data.extend_from_slice(&message_counter.to_le_bytes());
	blake2_256(&data)
}

fn channel_for_request(
	sender: &sr25519::Public,
	receiver: &sr25519::Public,
	counter: u64,
) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(b"request");
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	data.extend_from_slice(&counter.to_le_bytes());
	blake2_256(&data)
}

fn channel_for_response(
	sender: &sr25519::Public,
	receiver: &sr25519::Public,
	counter: u64,
) -> Topic {
	let mut data = Vec::new();
	data.extend_from_slice(b"response");
	data.extend_from_slice(sender.as_ref());
	data.extend_from_slice(receiver.as_ref());
	data.extend_from_slice(&counter.to_le_bytes());
	blake2_256(&data)
}
