use std::{
	collections::HashMap, env, io::{self, Write}, time::Duration
};

use codec::{Decode, Encode};
use hex_literal::hex;
use log::{debug, warn};
use polkadot_primitives::Slot;
use sc_chain_spec::resolve_state_version_from_wasm;
use sc_client_api::{Backend, BlockImportOperation};
use sc_consensus::{
	block_import::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult},
	StateAction,
};
// use sp_core::bytes::from_hex;
// use sp_runtime::offchain::storage;

use sc_executor::WasmExecutor;
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header, PhantomData};
use sp_storage::{ChildInfo, ChildType, PrefixedStorageKey, StorageChild};
use tokio::{io::AsyncWriteExt, time::sleep};
use futures::channel::oneshot::Receiver;
use sp_core::traits::SpawnEssentialNamed;

const LOG_TARGET: &str = "doppelganger";

// pub mod overrides;

#[derive(Debug, Clone, Default)]
struct OverrideKeys {
	pub(crate) overrides: HashMap<Vec<u8>, Vec<u8>>,
	pub(crate) injects: HashMap<Vec<u8>, Vec<u8>>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DoppelGangerContext {
	Relaychain,
	Parachain,
}

impl std::fmt::Display for DoppelGangerContext {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let printable = match *self {
			DoppelGangerContext::Relaychain => "relay",
			DoppelGangerContext::Parachain => "para",
		};
		write!(f, "{}", printable)
	}
}

pub async fn teardown_com(rx: Receiver<()>) {
	let _ = rx.await;
	warn!("🪞 Shutdown received, waiting 6s before shutdown");
	sleep(Duration::from_secs(6)).await;
	warn!("🪞 Shutting down now");
}

pub struct DoppelGangerBlockImport<BI, Block>
where
	Block: BlockT,
	BI: BlockImport<Block>,
{
	inner: BI,
	context: DoppelGangerContext,
	spawner: Box<dyn SpawnEssentialNamed>,
	_phantom: PhantomData<Block>,
}

impl<Block: BlockT, BI: BlockImport<Block>> DoppelGangerBlockImport<BI, Block> {
	pub fn new(inner: BI, context: DoppelGangerContext, spawner: impl SpawnEssentialNamed + 'static) -> Self {
		println!("Wrapping with DoppelGangerBlockImport");
		DoppelGangerBlockImport { inner, context, spawner: Box::new(spawner), _phantom: PhantomData }
	}
}


#[async_trait::async_trait]
impl<Block, BI> BlockImport<Block> for DoppelGangerBlockImport<BI, Block>
where
	Block: BlockT,
	BI: BlockImport<Block> + Send + Sync,
{
	type Error = BI::Error;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		mut block: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		let mut dump: HashMap<String, String> = HashMap::default();
		let dump_file = if std::env::var("ZOMBIE_DUMP").is_ok() {
			let file_path = format!("/tmp/dump_{}.json", self.context);
			println!("dump_file: {}", file_path);
			let file = tokio::fs::OpenOptions::new()
				.create(true)
				.write(true)
				.open(file_path)
				.await
				.unwrap();
			Some(file)
		} else {
			None
		};

		let number = *block.header.number();
		if block.with_state() {
			// spawn an essential task to gracefully shutdown the node later
			let (doppelganger_tx, doppelganger_rx) = futures::channel::oneshot::channel();
			self.spawner.spawn_essential("doppelganger-worker", Some("doppelganger"), Box::pin(teardown_com(doppelganger_rx)));
			debug!(target: LOG_TARGET, "Block param with state, with header {:?}", block.header);
			if let StateAction::ApplyChanges(sc_consensus::StorageChanges::Import(
				ref mut imported_state,
			)) = block.state_action
			{
				let mut storage = sp_storage::Storage::default();

				let override_keys: OverrideKeys = match self.context {
					DoppelGangerContext::Relaychain => get_overrides().await,
					DoppelGangerContext::Parachain => get_overrides_para(1000_u32).await,
				};

				let overrides: HashMap<Vec<u8>, Vec<u8>> = override_keys.overrides;
				let injects: HashMap<Vec<u8>, Vec<u8>> = override_keys.injects;

				let session_current_index_key: Vec<u8> =
					hex!("cec5070d609dd3497f72bde07fc96ba072763800a36a99fdfc7c10f6415f6ee6").into();
				const SESSION_NEXT_KEYS_PREFIX: [u8; 32] =
					hex!("cec5070d609dd3497f72bde07fc96ba04c014e6bf8b8c2c011e7290b85696bb3");
				const COLLATORSELECTION_CANDIDATELIST: [u8; 32] =
					hex!("15464cac3378d46f113cd5b7a4d71c84ad588da1c23d1f764a5ff7b71e776f5a");

				const CORE_ASSIGNMENT_PROVIDER_CORE_SCHEDULES_PREFIX: [u8; 32] =
					hex!("638595eebaa445ce03a13547bece90e74a4aebd4fb28ddd34de9226f0abce904");
				// let para_session_info_prefix: Vec<u8> =
				// hex!("4da2c41eaffa8e1a791c5d65beeefd1f028685274e698e781f7f2766cba0cc83").into();
				let paras_heads_prefix: Vec<u8> =
					hex!("cd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3").into();
				let mut injects_iter = injects.clone().into_iter();
				{
					for state in imported_state.state.0.iter_mut() {
						debug!(target: LOG_TARGET,
							"parent_storage_keys: {:?}",
							state.parent_storage_keys
						);
						debug!(target: LOG_TARGET,"state_root: {:?}", state.state_root);

						if state.parent_storage_keys.len() == 0 && state.state_root.len() == 0 {
							// AHM
							const AMOUNT_OF_DOTS_TO_MOVE: u128 = 10000000000000_u128;
							let account_to_subtract_k: Vec<u8> = hex!("26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da91cdb29d91f7665b36dc5ec5903de32467628a5be63c4d3c8dbb96c2904b1a9682e02831a1af836c7efc808020b92fa63").into();
							let account_alice_k: Vec<u8> = hex!("26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9de1e86a9a8c739864cf3cc5ec2bea59fd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d").into();
							{
								// inject 10 dots to alice
								state.key_values.push((
									account_alice_k.clone(),
									default_with_amount_free(AMOUNT_OF_DOTS_TO_MOVE),
								));
							}

							// hack for para override wasm
							// TODO: needs to refactor
							let current_code_para_1000_k: Vec<u8> = hex!("cd710b30bd2eab0352ddcc26417aa194e2d1c22ba0a888147714a3487bd51c63b6ff6f7d467b87a9e8030000").into();
							let para_1000_code_hash_kv = state
								.key_values
								.iter()
								.find(|(k, _v)| k == &current_code_para_1000_k);

							let mut session_current_index_value: Vec<u8> = vec![];
							// DO NOT override paraSessionInfo anymore
							// we store the sessionInfo to override the current index later
							// let mut session_info = vec![];

							const GENESIS_SLOT_KEY: [u8; 32] = hex!(
								"1cb6f36e027abb2091cfb5110ab5087f678711d15ebbceba5cd0cea158e6675a"
							);
							const CURRENT_SLOT_KEY: [u8; 32] = hex!(
								"1cb6f36e027abb2091cfb5110ab5087f06155b3cd9a8c9e5e9a23fd5dc13a5ed"
							);
							const CURRENT_EPOCH_INDEX: [u8; 32] = hex!(
								"1cb6f36e027abb2091cfb5110ab5087f38316cbf8fa0da822a20ac1c55bf1be3"
							);

							let code_refs_prefix =
								"cd710b30bd2eab0352ddcc26417aa1948c27d984a48a10b1ebf28036a4a4444b";
							let code_by_hash_ref =
								"cd710b30bd2eab0352ddcc26417aa194383e6dcb39e0be0a2e6aeb8b94951ab6";

							let mut genesis_slot_value: Vec<u8> = vec![];
							let mut current_slot_value: Vec<u8> = vec![];
							let mut current_epoch_value: Vec<u8> = vec![];
							state.key_values = state.key_values.iter().filter_map( |(key, value)| {
								let key_hex = sp_core::hexdisplay::HexDisplay::from(key);
								let value_hex = sp_core::hexdisplay::HexDisplay::from(value);

								// dump if needed
								if dump_file.is_some() {
									dump.insert(hex::encode(key), hex::encode(value));
								}
								if key == &session_current_index_key {
									session_current_index_value = value.clone();
								}

								if key == &GENESIS_SLOT_KEY {
									genesis_slot_value = value.clone();
									// Skip now, we will override later to produce a new session in 10 blocks
									return None;
								}

								if key == &CURRENT_SLOT_KEY {
									current_slot_value = value.clone()
								}

								if key == &CURRENT_EPOCH_INDEX {
									current_epoch_value = value.clone()
								}

								// skip collatorSelection_candidateList since we
								// want only invulnerables in the set
								if key == &COLLATORSELECTION_CANDIDATELIST {
									debug!(target: LOG_TARGET, "skipping collatorSelection candidateList... old key: {}", hex::encode(key));
									return None;
								}

								// skip Session NextKeys entries
								if key.starts_with(&SESSION_NEXT_KEYS_PREFIX) {
									debug!(target: LOG_TARGET, "skipping Session NextKey... old key: {}", hex::encode(key));
									return None;
								}

								// skip coretimeAssignmentProvider.coreSchedules
								// since we want an empty list
								if key.starts_with(&CORE_ASSIGNMENT_PROVIDER_CORE_SCHEDULES_PREFIX) {
									debug!(target: LOG_TARGET, "skipping coretimeAssignmentProvider CoreSchedules... old key: {}", hex::encode(key));
									return None;
								}


								// DO NOT OVERRIDE paraSessionInfo anymore
								// if key.starts_with(&para_session_info_prefix) {
								// 	session_info.push((key.clone(), value.clone()));
								// 	// skipped for now
								// 	return None;
								// }


								if let DoppelGangerContext::Relaychain = self.context {
									let para_1000_code_hash_kv = para_1000_code_hash_kv.unwrap();
									let code_refs_prefix_1000 = format!("{code_refs_prefix}{}", hex::encode(&para_1000_code_hash_kv.1));
									let code_refs_prefix_1000_k = hex::decode(code_refs_prefix_1000).unwrap();

									let code_by_hash_ref_prefix_1000 = format!("{code_by_hash_ref}{}", hex::encode(&para_1000_code_hash_kv.1));
									let code_by_hash_ref_prefix_1000_k = hex::decode(code_by_hash_ref_prefix_1000).unwrap();

									if key == &code_refs_prefix_1000_k {
										let inject_kv = injects_iter.find(|(k,_v)| k.starts_with(&hex!("cd710b30bd2eab0352ddcc26417aa1948c27d984a48a10b1ebf28036a4a4444b")));
										if let Some((k,v)) = inject_kv {
											debug!(target: LOG_TARGET, "code_refs(1000) old key: {}", hex::encode(key));
											debug!(target: LOG_TARGET, "code_refs(1000) old value: {}", hex::encode(value));
											debug!(target: LOG_TARGET,"code_refs(1000) new key: {}", hex::encode(&k));
											debug!(target: LOG_TARGET,"code_refs(1000) new value: {}", hex::encode(&v));
											storage.top.insert(k.clone(), v.clone());
											return Some((k.clone(), v.clone()))
										}
									}

									if key == &code_by_hash_ref_prefix_1000_k {
										let inject_kv = injects_iter.find(|(k,_v)| k.starts_with(&hex!("cd710b30bd2eab0352ddcc26417aa194383e6dcb39e0be0a2e6aeb8b94951ab6")));
										if let Some((k,v)) = inject_kv {
											debug!(target: LOG_TARGET, "code_by_hash(1000) old key: {}", hex::encode(key));
											debug!(target: LOG_TARGET, "code_by_hash(1000) old value: {}", HexDisplay::from(value));
											debug!(target: LOG_TARGET,"code_by_hash(1000) new key: {}", hex::encode(&k));
											debug!(target: LOG_TARGET,"code_by_hash(1000) new value: {}", HexDisplay::from(&v));
											storage.top.insert(k.clone(), v.clone());
											return Some((k.clone(), v.clone()))
										}
									}

									// AHM (move 10 dots to alice)
									if key == &account_to_subtract_k {
										debug!(target: LOG_TARGET, "Moving 10 dots from: {} to alice", HexDisplay::from(&account_to_subtract_k));
										let new_value = subtract_free_balance_from_state(value, AMOUNT_OF_DOTS_TO_MOVE);

										storage.top.insert(key.clone(), new_value.clone());
										return Some((key.clone(), new_value.clone()))
									}
								}


								if let Some(override_value) = overrides.get(key) {
									debug!(target: LOG_TARGET, "Overriding key: {}",key_hex);
									if &key_hex.to_string() == "3a636f6465" {
										debug!(target: LOG_TARGET, "old value: {}", hex::encode(value));
										debug!(target: LOG_TARGET,"new value: {}", hex::encode(override_value));
									} else {
										debug!(target: LOG_TARGET, "old value: {}", value_hex);
										debug!(target: LOG_TARGET,"new value: {}", sp_core::hexdisplay::HexDisplay::from(override_value));
									}
									// storage.top.
									storage.top.insert(key.clone(), override_value.clone());
									return Some((key.clone(), override_value.clone()))
								} else {
									// check if we need to remove the key
									// paras.heads
									if key.starts_with(&paras_heads_prefix) {
										None
									} else {
										// insert the value as is
										storage.top.insert(key.clone(), value.clone());
										Some((key.clone(), value.clone()))
									}
								}
							}).collect();

							// DO NOT OVERRIDE paraSessionInfo anymore
							// here we need to find and override the current session index
							// let para_session_info_current_key =
							// [para_session_info_prefix.clone(),
							// session_current_index_value].concat(); for (k,v) in
							// session_info.into_iter() { 	// not override session info
							// 	if k == para_session_info_current_key {
							// 		// override
							// 		let overrided_session_info: Vec<u8> =
							// hex!("10020000000300000000000000010000005e80553a1476e73ed335fce7ec974e2edfea0a69929ac65608a2e246d08fc561060000001090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a481090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a481090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48041000000000010000000200000003000000010000000000000002000000190000000200000002000000"
							// ).into(); 		storage.top.insert(k.clone(),
							// overrided_session_info.clone()); 		state.key_values.push((k,
							// overrided_session_info));

							// 	} else {
							// 		storage.top.insert(k.clone(), v.clone());
							// 		state.key_values.push((k,v));
							// 	}
							// }

							// calculate genesis slot in order to force session change
							if self.context == DoppelGangerContext::Relaychain {
								let genesis_slot_override =
									calculate_genesis_slot(current_slot_value, current_epoch_value);
								storage.top.insert(
									GENESIS_SLOT_KEY.into(),
									genesis_slot_override.encode(),
								);
								state.key_values.push((
									GENESIS_SLOT_KEY.into(),
									genesis_slot_override.encode(),
								));
							}

							// Injects keys left
							for (k, v) in injects.iter() {
								debug!(target: LOG_TARGET, "Injecting key: {}", sp_core::hexdisplay::HexDisplay::from(k));
								debug!(target: LOG_TARGET,"Injecting value: {}", sp_core::hexdisplay::HexDisplay::from(v));
								storage.top.insert(k.clone(), v.clone());
								state.key_values.push((k.clone(), v.clone()));
							}
						} else {
							for parent_storage in &state.parent_storage_keys {
								let storage_key = PrefixedStorageKey::new_ref(&parent_storage);
								let storage_key = match ChildType::from_prefixed_key(&storage_key) {
									Some((ChildType::ParentKeyId, storage_key)) => storage_key,
									None => panic!("Invalid child storage key!"),
								};
								let entry = storage
									.children_default
									.entry(storage_key.to_vec())
									.or_insert_with(|| StorageChild {
										data: Default::default(),
										child_info: ChildInfo::new_default(storage_key),
									});
								for (key, value) in state.key_values.iter_mut() {
									if let Some(override_value) = overrides.get(key) {
										println!(
											"Overriding key (in child): {}",
											sp_core::hexdisplay::HexDisplay::from(key)
										);
										println!(
											"old value (in child): {}",
											sp_core::hexdisplay::HexDisplay::from(value)
										);
										*value = override_value.clone();
										println!(
											"new value (in child): {}",
											sp_core::hexdisplay::HexDisplay::from(value)
										);
									}
									entry.data.insert(key.clone(), value.clone());
								}
							}
						}
					}
				}

				if let Some(mut file) = dump_file {
					let dump_json =
						serde_json::to_string_pretty(&dump).expect("serialize should work");
					file.write_all(dump_json.as_bytes()).await.expect("write should work");
					file.flush().await.expect("flush should work");
				}

				let backend = sc_client_api::in_mem::Backend::<Block>::new();
				let mut op = backend
					.begin_operation()
					.expect("create BlockImportOperation should not fail.");

				let executor: WasmExecutor<(
					cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
					sp_io::SubstrateHostFunctions,
				)> = WasmExecutor::builder().build();

				let state_version =
					resolve_state_version_from_wasm::<_, HashingFor<Block>>(&storage, &executor)
						.expect("get state_version from storage should works.");

				let state_root =
					op.reset_storage(storage, state_version).expect("reset storage should work.");

				debug!(target: LOG_TARGET, "new state_version {:?}", state_version);
				debug!(target: LOG_TARGET,"new state_root: {:?}", state_root);
				block.header.set_state_root(state_root);
				block.post_hash = Some(block.post_header().hash());
				// NOT create gap
				block.create_gap = false;
			}

			let para_head = block.header.encode();
			let block_hash = block.header.hash();
			let res = self.inner.import_block(block).await;
			println!("Block import done! : {:?}, killing the process", res);
			// use last line to share the block number
			println!("number: {}, hash: {}", number, block_hash);
			if let DoppelGangerContext::Parachain = self.context {
				let output_buf = format!("{}", HexDisplay::from(&para_head)).into_bytes();
				if let Ok(para_head_path) = env::var("ZOMBIE_PARA_HEAD_PATH") {
					let err_msg =
						format!("write to 'ZOMBIE_PARA_HEAD_PATH'= {para_head_path} should work");
					tokio::fs::write(para_head_path, output_buf).await.expect(&err_msg);
				} else {
					// send to stdout
					io::stdout().write_all(&output_buf).expect("write to stdout should work");
				}
			}

			// store the block number
			if let Ok(zombie_info_path) = env::var("ZOMBIE_INFO_PATH") {
				let err_msg =
					format!("write to 'ZOMBIE_INFO_PATH'= {zombie_info_path} should work");
				tokio::fs::write(zombie_info_path, format!("{number}")).await.expect(&err_msg);
			} else {
				// send to stdout
				io::stdout()
					.write_all(&format!("{number}").as_bytes())
					.expect("write to stdout should work");
			}

			if std::env::var("ZOMBIE_KEEP_ALIVE_ON_SYNC").is_ok() {
				return res;
			}

			if doppelganger_tx.send(()).is_err() {
				warn!("Error sending msg to gracefully shutdown, killing process...");
				std::process::exit(1);
			} else {
				return res;
			}
		}

		return self.inner.import_block(block).await
	}
}

// Default keys used for Alice/Bob
async fn get_overrides() -> OverrideKeys {
	let override_keys: OverrideKeys = if let Ok(overrides_path) =
		std::env::var(format!("ZOMBIE_RC_OVERRIDES_PATH"))
	{
		let content: HashMap<String, HashMap<String, String>> = serde_json::from_str(&tokio::fs::read_to_string(&overrides_path).await.expect(&format!("Overrides path 'ZOMBIE_PARA_OVERRIDES_PATH' ({overrides_path}) should be valid. qed"))).expect("Should be a valid json");
		let mut overrides: HashMap<Vec<u8>, Vec<u8>> = Default::default();
		let mut injects: HashMap<Vec<u8>, Vec<u8>> = Default::default();

		let para_head_keys = [
			// asset-hub (1000)
			"cd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000"
		];
		for key in para_head_keys {
			if let Ok(para_head) = std::env::var(format!("ZOMBIE_{}", key)) {
				overrides.insert(
					hex::decode(key).expect("para_head_key should be valid").into(),
					hex::decode(para_head).expect("para_head_value should be valid").into(),
				);
			}
		}

		if let Some(inner) = content.get("overrides") {
			for (key, value) in inner {
				overrides.insert(
					hex::decode(key).expect("key should be valid hex").into(),
					hex::decode(value).expect("value should be valid hex").into(),
				);
			}
		}

		if let Some(inner) = content.get("injects") {
			for (key, value) in inner {
				injects.insert(
					hex::decode(key).expect("key should be valid hex").into(),
					hex::decode(value).expect("value should be valid hex").into(),
				);
			}
		}

		OverrideKeys { overrides, injects }
	} else {
		OverrideKeys::default()
	};

	override_keys
}

async fn get_overrides_para(_id: u32) -> OverrideKeys {
	let override_keys: OverrideKeys = if let Ok(overrides_path) =
		std::env::var(format!("ZOMBIE_PARA_OVERRIDES_PATH"))
	{
		let content: HashMap<String, HashMap<String, String>> = serde_json::from_str(&tokio::fs::read_to_string(&overrides_path).await.expect(&format!("Overrides path 'ZOMBIE_PARA_OVERRIDES_PATH' ({overrides_path}) should be valid. qed"))).expect("Should be a valid json");
		let mut overrides: HashMap<Vec<u8>, Vec<u8>> = Default::default();
		let mut injects: HashMap<Vec<u8>, Vec<u8>> = Default::default();

		if let Some(inner) = content.get("overrides") {
			for (key, value) in inner {
				overrides.insert(
					hex::decode(key).expect("key should be valid hex").into(),
					hex::decode(value).expect("value should be valid hex").into(),
				);
			}
		}

		if let Some(inner) = content.get("injects") {
			for (key, value) in inner {
				injects.insert(
					hex::decode(key).expect("key should be valid hex").into(),
					hex::decode(value).expect("value should be valid hex").into(),
				);
			}
		}

		OverrideKeys { overrides, injects }
	} else {
		OverrideKeys::default()
	};

	override_keys
}

fn calculate_genesis_slot(current: Vec<u8>, epoch_idx: Vec<u8>) -> Slot {
	const DEFAULT_EPOCH_DURARION: u64 = 2400;

	let epoch_duration: u64 = std::env::var("ZOMBIE_RC_EPOCH_DURATION")
		.unwrap_or(DEFAULT_EPOCH_DURARION.to_string())
		.parse()
		.expect("ZOMBIE_RC_EPOCH_DURATION should be a valid u64");
	let diff_target = epoch_duration - 10;
	let current_slot: Slot = Decode::decode(&mut current.as_slice()).unwrap();
	let epoch_index: u64 = Decode::decode(&mut epoch_idx.as_slice()).unwrap();
	let genesis_slot = current_slot.saturating_sub((epoch_index * epoch_duration) + diff_target);
	genesis_slot
}

// AHM
use frame_system::AccountInfo;
use pallet_balances::AccountData;

fn subtract_free_balance_from_state(data: &Vec<u8>, amount: u128) -> Vec<u8> {
	let mut account_info =
		AccountInfo::<u32, AccountData<u128>>::decode(&mut data.as_slice()).unwrap();
	debug!(target: LOG_TARGET, "AccountInfo to subtract: {:?} ", account_info);
	account_info.data.free -= amount;
	account_info.encode()
}

fn default_with_amount_free(amount: u128) -> Vec<u8> {
	let mut account = AccountInfo::<u32, AccountData<u128>>::default();
	account.data.free = amount;
	account.providers = 1;
	account.encode()
}

#[cfg(test)]
mod tests {
	use super::{calculate_genesis_slot, *};
	use codec::{Decode, Encode};
	use polkadot_primitives::Slot;

	#[test]
	#[ignore]
	fn account_info_should_works() {
		let data: Vec<u8>  = hex::decode("2900000002000000010000000000000070a95481242d000000000000000000000086c46b5d000000000000000000000000203d88792d0000000000000000000000000000000000000000000000000080").unwrap();
		let account: AccountInfo<u32, AccountData<u128>> =
			AccountInfo::<u32, AccountData<u128>>::decode(&mut data.as_slice()).unwrap();
		println!("{account:?}");
		let new_account_data = subtract_free_balance_from_state(&data, 10_000_000_000_000_u128);
		let new_account: AccountInfo<u32, AccountData<u128>> =
			AccountInfo::<u32, AccountData<u128>>::decode(&mut new_account_data.as_slice())
				.unwrap();
		println!("{new_account:?}");
	}

	#[test]
	#[ignore]
	fn calculate_slot() {
		const EPOCH_DURARION: u64 = 2400;
		let current_slot: Slot =
			Decode::decode(&mut hex::decode("daf0341100000000").unwrap().as_slice()).unwrap();
		let epoch_index: u64 =
			Decode::decode(&mut hex::decode("6826000000000000").unwrap().as_slice()).unwrap();
		println!("{current_slot}");

		let slots = EPOCH_DURARION * epoch_index;

		let genesis = calculate_genesis_slot(
			hex::decode("daf0341100000000").unwrap(),
			hex::decode("6826000000000000").unwrap(),
		);

		println!("current_slot: {current_slot}");
		println!("epoch_index: {epoch_index}");
		println!("genesis: {genesis}");
		println!("slots: {slots}");

		let diff = current_slot - (genesis.saturating_add(slots));

		println!("diff: {diff}");

		println!("encoded: {:?}", sp_core::hexdisplay::HexDisplay::from(&genesis.encode()));
	}

	#[test]
	#[ignore = "needs dump"]
	fn encode_works() {
		// let value_hex = include_str!("session_info_prod.hex");
		let value_hex = include_str!("session_info_modified.hex");

		use polkadot_runtime_parachains::configuration::HostConfiguration;
		type BlockNumber = u32;

		let data = hex::decode(value_hex).unwrap();
		let s: polkadot_primitives::SessionInfo =
			codec::Decode::decode(&mut data.as_slice()).unwrap();
		println!("{:#?}", s);

		let host_config_value_hex = "0000300000500000aaaa020000001000fbff0000100000000a000000403800005802000003000000020000000000500000c800008000000000e8764817000000000000000000000000e87648170000000000000000000000e80300000090010080000000009001000c01002000000600c4090000000000000601983a000000000000403800000006000000580200000300000059000000000000001e000000060000000200000014000000020000000803060000000a0000000a0000000105000000020000003e00000000000000f401000080b2e60e80c3c90180b2e60e00000000000000000000000005000000";
		let data = hex::decode(host_config_value_hex).unwrap();
		let mut host_config: HostConfiguration<BlockNumber> =
			codec::Decode::decode(&mut data.as_slice()).unwrap();
		host_config.scheduler_params.lookahead = 1;
		host_config.scheduler_params.num_cores = 1;
		host_config.scheduler_params.group_rotation_frequency = 20;
		host_config.scheduler_params.paras_availability_period = 4;
		host_config.approval_voting_params.max_approval_coalesce_count = 1;
		host_config.needed_approvals = 2;
		host_config.n_delay_tranches = 25;
		host_config.relay_vrf_modulo_samples = 2;
		host_config.minimum_backing_votes = 1;
		println!("{:?}", host_config);
		println!("encoded: \n{}", sp_core::hexdisplay::HexDisplay::from(&host_config.encode()));
	}
}
