use std::collections::HashMap;
use std::io::{self, Write};
use std::env;

use codec::Decode;
use hex_literal::hex;
use log::debug;
use polkadot_primitives::Slot;
use sc_client_api::{Backend, BlockImportOperation};
use sc_consensus::{
	block_import::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult},
	StateAction,
};
use sp_core::bytes::from_hex;
use sp_runtime::offchain::storage;
use sp_runtime::traits::{Block as BlockT, Header, PhantomData};
use sp_storage::{ChildInfo, ChildType, PrefixedStorageKey, StorageChild};
use sp_core::{hexdisplay::HexDisplay, Encode};
use tokio::io::AsyncWriteExt;

const LOG_TARGET: &str = "doppelganger";

pub mod overrides;


#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DoppelGangerContext {
	Relaychain,
	Parachain
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

pub struct DoppelGangerBlockImport<BI, Block>
where
	Block: BlockT,
	BI: BlockImport<Block>,
{
	inner: BI,
	context: DoppelGangerContext,
	_phantom: PhantomData<Block>,
}

impl<Block: BlockT, BI: BlockImport<Block>> DoppelGangerBlockImport<BI, Block> {
	pub fn new(inner: BI, context: DoppelGangerContext) -> Self {
		println!("Wrapping with DoppelGangerBlockImport");
		DoppelGangerBlockImport { inner, context, _phantom: PhantomData }
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
			let mut file = tokio::fs::OpenOptions::new()
			.create(true)
			.write(true)
			.open(file_path).await.unwrap();
			Some(file)
		} else {
			None
		};

		let number = *block.header.number();
		if block.with_state() {
			debug!(target: LOG_TARGET, "Block param with state, with header {:?}", block.header);
			if let StateAction::ApplyChanges(sc_consensus::StorageChanges::Import(
				ref mut imported_state,
			)) = block.state_action
			{
				let mut storage = sp_storage::Storage::default();
				let overrides = match self.context {
					DoppelGangerContext::Relaychain => get_overrides().await,
					DoppelGangerContext::Parachain => get_overrides_para(1000_u32).await,
				};
				let session_current_index_key: Vec<u8> = hex!("cec5070d609dd3497f72bde07fc96ba072763800a36a99fdfc7c10f6415f6ee6").
				into();
				// let para_session_info_prefix: Vec<u8> = hex!("4da2c41eaffa8e1a791c5d65beeefd1f028685274e698e781f7f2766cba0cc83").into();
				let paras_heads_prefix: Vec<u8> = hex!("cd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3").into();
				{

					for state in imported_state.state.0.iter_mut() {
						debug!(target: LOG_TARGET,
							"parent_storage_keys: {:?}",
							state.parent_storage_keys
						);
						debug!(target: LOG_TARGET,"state_root: {:?}", state.state_root);


						if state.parent_storage_keys.len() == 0 && state.state_root.len() == 0 {
							let mut session_current_index_value: Vec<u8> = vec![];
							// DO NOT override paraSessionInfo anymore
							// we store the sessionInfo to override the current index later
							// let mut session_info = vec![];
							const GENESIS_SLOT_KEY: [u8;32] = hex!("1cb6f36e027abb2091cfb5110ab5087f678711d15ebbceba5cd0cea158e6675a");
							const CURRENT_SLOT_KEY: [u8;32] = hex!("1cb6f36e027abb2091cfb5110ab5087f06155b3cd9a8c9e5e9a23fd5dc13a5ed");
							const CURRENT_EPOCH_INDEX: [u8;32] = hex!("1cb6f36e027abb2091cfb5110ab5087f38316cbf8fa0da822a20ac1c55bf1be3");

							let mut genesis_slot_value: Vec<u8> = vec![];
							let mut current_slot_value: Vec<u8> = vec![];
							let mut current_epoch_value: Vec<u8> = vec![];
							state.key_values = state.key_values.iter().filter_map( |(key, value)| {
								let key_hex = sp_core::hexdisplay::HexDisplay::from(key);
								let value_hex = sp_core::hexdisplay::HexDisplay::from(value);

								// dump if needed
								if dump_file.is_some() {
									dump.insert(key_hex.to_string(), value_hex.to_string());
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

								// DO NOT OVERRIDE paraSessionInfo anymore
								// if key.starts_with(&para_session_info_prefix) {
								// 	session_info.push((key.clone(), value.clone()));
								// 	// skipped for now
								// 	return None;
								// }


								if let Some(override_value) = overrides.get(key) {
									debug!(target: LOG_TARGET, "Overriding key: {}",key_hex);
									debug!(target: LOG_TARGET, "old value: {}", value_hex);
									debug!(target: LOG_TARGET,"new value: {}", sp_core::hexdisplay::HexDisplay::from(override_value));
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
							// let para_session_info_current_key = [para_session_info_prefix.clone(), session_current_index_value].concat();
							// for (k,v) in session_info.into_iter() {
							// 	// not override session info
							// 	if k == para_session_info_current_key {
							// 		// override
							// 		let overrided_session_info: Vec<u8> = hex!("10020000000300000000000000010000005e80553a1476e73ed335fce7ec974e2edfea0a69929ac65608a2e246d08fc561060000001090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a481090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a481090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48041000000000010000000200000003000000010000000000000002000000190000000200000002000000").into();
							// 		storage.top.insert(k.clone(), overrided_session_info.clone());
							// 		state.key_values.push((k,overrided_session_info));

							// 	} else {
							// 		storage.top.insert(k.clone(), v.clone());
							// 		state.key_values.push((k,v));
							// 	}
							// }

							// calculate genesis slot in order to force session change
							if self.context == DoppelGangerContext::Relaychain {
								let genesis_slot_override = calculate_genesis_slot(current_slot_value, current_epoch_value);
								storage.top.insert(GENESIS_SLOT_KEY.into(), genesis_slot_override.encode());
								state.key_values.push((GENESIS_SLOT_KEY.into(), genesis_slot_override.encode()));
							}
						} else {
							for parent_storage in &state.parent_storage_keys {
								let storage_key = PrefixedStorageKey::new_ref(&parent_storage);
								let storage_key =
									match ChildType::from_prefixed_key(&storage_key) {
										Some((ChildType::ParentKeyId, storage_key)) =>
											storage_key,
										None =>
											panic!("Invalid child storage key!"),
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
					let dump_json = serde_json::to_string_pretty(&dump).expect("serialize should work");
					file.write_all(dump_json.as_bytes()).await.expect("write should work");
					file.flush().await.expect("flush should work");
				}


				let backend = sc_client_api::in_mem::Backend::<Block>::new();
				let mut op = backend.begin_operation().expect("create BlockImportOperation should not fail.");
				let state_version = if storage.children_default.len() > 0 {
					sp_storage::StateVersion::V1
				} else {
					sp_storage::StateVersion::V0
				};

				let state_root = op.reset_storage(storage, state_version ).expect("reset storage should work.");

				block.header.set_state_root(state_root);
				block.post_hash = Some(block.post_header().hash());
				// NOT create gap
				block.create_gap = false;
			}

			let para_head = block.header.encode();
			let res = self.inner.import_block(block).await;
			println!("Block import done! : {:?}, killing the process", res);
			// use last line to share the block number
			println!("{}", number);
			if let DoppelGangerContext::Parachain = self.context {
				let output_buf = format!("{}",HexDisplay::from(&para_head)).into_bytes();
				if let Ok(para_head_path) = env::var("ZOMBIE_PARA_HEAD_PATH") {
					let err_msg = format!("write to 'ZOMBIE_PARA_HEAD_PATH'= {para_head_path} should work");
					tokio::fs::write(para_head_path, output_buf).await.expect(&err_msg);
				} else {
					// send to stdout
					io::stdout().write_all(&output_buf).expect("write to stdout should work");
				}
			}

			if std::env::var("ZOMBIE_KEEP_ALIVE_ON_SYNC").is_ok() {
				return res;

			}

			// KILL by default
			std::process::exit(if res.is_ok() { 0 } else { 1 });
		}

		return self.inner.import_block(block).await
	}
}

// Default keys used for Alice/Bob
async fn get_overrides() -> HashMap<Vec<u8>, Vec<u8>> {
	let mut overrides: HashMap<Vec<u8>, Vec<u8>> = if let Ok(overrides_path) = std::env::var(format!("ZOMBIE_RC_OVERRIDES_PATH")) {
		let content: HashMap<String, String> = serde_json::from_str(&tokio::fs::read_to_string(overrides_path).await.expect("Overrides path 'ZOMBIE_RC_OVERRIDES_PATH' should be valid. qed")).expect("Should be a valid json");
		let mut overrides: HashMap<Vec<u8>, Vec<u8>> = Default::default();
		for (key, value) in content {
			overrides.insert(
				hex::decode(key).expect("key should be valid hex").into(),
				hex::decode(value).expect("value should be valid hex").into()
			);
		}
		overrides
	} else {
		HashMap::from([
			// <Pallet> < Item>
			// Validator Validators
			(
				hex!("7d9fe37370ac390779f35763d98106e888dcde934c658227ee1dfafcd6e16903").into(),
				hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e").into()
			),
			// Session Validators (alice, bob)
			(
				hex!("cec5070d609dd3497f72bde07fc96ba088dcde934c658227ee1dfafcd6e16903").into(),
				//(alice, bob)
				hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e").into()
			),
			//	Session QueuedKeys (alice, bob)
			(
				hex!("cec5070d609dd3497f72bde07fc96ba0e0cdd062e6eaf24295ad4ccfc41d4609").into(),
				// (alice, bob)
				hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0eed43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1fe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860ed17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae698eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27").into()
			),
			// Babe Authorities (alice, bob)
			(
				hex!("1cb6f36e027abb2091cfb5110ab5087f5e0621c4869aa60c02be9adcc98a0d1d").into(),
				// (alice, bob)
				hex!("08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d01000000000000008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480100000000000000").into()
			),
			// Babe NextAuthorities (alice, bob)
			(
				hex!("1cb6f36e027abb2091cfb5110ab5087faacf00b9b41fda7a9268821c2a2b3e4c").into(),
				// (alice, bob)
				hex!("08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d01000000000000008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480100000000000000").into()
			),
			// Grandpa Authorities (alice, bob, charly, dave)
			(
				hex!("5f9cc45b7a00c5899361e1c6099678dc5e0621c4869aa60c02be9adcc98a0d1d").into(),
				// (alice, bob)
				hex!("0888dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee0100000000000000d17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae690100000000000000").into()
			),
			// Staking ForceEra
			(
				hex!("5f3e4907f716ac89b6347d15ececedcaf7dad0317324aecae8744b87fc95f2f3").into(),
				hex!("02").into()
			),

			// Staking Invulnerables (alice, bob)
			(
				hex!("5f3e4907f716ac89b6347d15ececedca5579297f4dfb9609e7e4c2ebab9ce40a").into(),
				hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e").into()
			),

			// System Accounts?

			// paras.parachains (only 1000)
			(
				hex!("cd710b30bd2eab0352ddcc26417aa1940b76934f4cc08dee01012d059e1b83ee").into(),
				hex!("04e8030000").into()
			),
			// paraScheduler.validatorGroup (one group of 4 validators)
			(
				hex!("94eadf0156a8ad5156507773d0471e4a16973e1142f5bd30d9464076794007db").into(),
				hex!("041000000000010000000200000003000000").into()
			),
			// paraScheduler.claimQueue (empty, will auto-fill?)
			(
				hex!("94eadf0156a8ad5156507773d0471e4a49f6c9aa90c04982c05388649310f22f").into(),
				hex!("040000000000").into()
			),
			// paraShared.activeValidatorIndices (4 validators)
			(
				hex!("b341e3a63e58a188839b242d17f8c9f82586833f834350b4d435d5fd269ecc8b").into(),
				// 2 validators
				hex!("080000000001000000").into()
			),
			// paraShared.activeValidatorKeys (4 validators, alice, bob)
			(
				hex!("b341e3a63e58a188839b242d17f8c9f87a50c904b368210021127f9238883a6e").into(),
				// (alice, bob)
				hex!("08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48").into()
			),
			// authorityDiscovery.keys (4 validators, alice, bob)
			(
				hex!("2099d7f109d6e535fb000bba623fd4409f99a2ce711f3a31b2fc05604c93f179").into(),
				// (alice, bob)
				hex!("08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48").into()
			),
			// authorityDiscovery.nextKeys (4 validators, alice, bob)
			(
				hex!("2099d7f109d6e535fb000bba623fd4404c014e6bf8b8c2c011e7290b85696bb3").into(),
				// (alice, bob)
				hex!("08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48").into()
			),

			// Core descriptor, ensure core 0 is asset-hub
			(
				hex!("638595eebaa445ce03a13547bece90e704e6ac775a3245623103ffec2cb2c92fb4def25cfda6ef3ac02a707a7013b12ddc9c5f6a3e1994c51754be175bd6a3d4").into(),
				hex!("00010402e803000000e100e100010000e1").into()
			),
			// dmp downwardMessageQueueHeads (empty for para 1000)
			(
				hex!("63f78c98723ddc9073523ef3beefda0c4d7fefc408aac59dbfe80a72ac8e3ce5b6ff6f7d467b87a9e8030000").into(),
				hex!("0000000000000000000000000000000000000000000000000000000000000000").into()
			),
			// hrmp.hrmpIngressChannelsIndex (empty for para 1000)
			(
				hex!("6a0da05ca59913bc38a8630590f2627c1d3719f5b0b12c7105c073c507445948b6ff6f7d467b87a9e8030000").into(),
				hex!("00").into()
			),
			// Configuration activeConfig
			(
				hex!("06de3d8a54d27e44a9d5ce189618f22db4b49d95320d9021994c850f25b8e385").into(),
				hex!("0000300000500000aaaa020000001000fbff0000100000000a000000403800005802000003000000020000000000500000c800008000000000e8764817000000000000000000000000e87648170000000000000000000000e80300000090010080000000009001000c01002000000600c4090000000000000601983a00000000000040380000000600000058020000030000001900000000000000020000000200000002000000140000000100000008030100000014000000040000000105000000010000000100000000000000f401000080b2e60e80c3c90180b2e60e00000000000000000000000005000000").into()
			),
			// paraScheduler availabilityCores (1 core, free)
			(
				hex!("94eadf0156a8ad5156507773d0471e4ab8ebad86f546c7e0b135a4212aace339").into(),
				hex!("0400").into()
			)
		])
	};

	let para_head_keys = [
		// asset-hub (1000)
		"cd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000"
	];
	for key in para_head_keys {
		if let Ok(para_head) = std::env::var(format!("ZOMBIE_{}", key)) {
			overrides.insert(
				hex::decode(key).expect("para_head_key should be valid").into(),
				hex::decode(para_head).expect("para_head_value should be valid").into()
			);
		}
	}

	overrides
}

async fn get_overrides_para(_id: u32) -> HashMap<Vec<u8>, Vec<u8>> {
	let overrides: HashMap<Vec<u8>, Vec<u8>> = if let Ok(overrides_path) = std::env::var(format!("ZOMBIE_PARA_OVERRIDES_PATH")) {
		let content: HashMap<String, String> = serde_json::from_str(&tokio::fs::read_to_string(&overrides_path).await.expect(&format!("Overrides path 'ZOMBIE_PARA_OVERRIDES_PATH' ({overrides_path}) should be valid. qed"))).expect("Should be a valid json");
		let mut overrides: HashMap<Vec<u8>, Vec<u8>> = Default::default();
		for (key, value) in content {
			overrides.insert(
				hex::decode(key).expect("key should be valid hex").into(),
				hex::decode(value).expect("value should be valid hex").into()
			);
		}
		overrides
	} else {
		// TODO: macth the id
		HashMap::from([
		// Session Validators
		(
			hex!("cec5070d609dd3497f72bde07fc96ba088dcde934c658227ee1dfafcd6e16903").into(),
			hex!("04005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15").into()
		),
		//	Session QueuedKeys
		(
			hex!("cec5070d609dd3497f72bde07fc96ba0e0cdd062e6eaf24295ad4ccfc41d4609").into(),
			hex!("04005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116").into()
		),

		// Session keys for `collator`
		(
			hex!("cec5070d609dd3497f72bde07fc96ba04c014e6bf8b8c2c011e7290b85696bb39af53646681828f1005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15").into(),
			hex!("eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116").into()
		),
		(
			hex!("cec5070d609dd3497f72bde07fc96ba0726380404683fc89e8233450c8aa1950eab3d4a1675d3d746175726180eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116").into(),
			hex!("005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15").into()
		),
		// CollatorSelection Invulnerables
		(
			hex!("15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a").into(),
			hex!("044cec53d80585625c427e909070de80016e629fa02e5cb373f3c4e94226417726").into()
		),
		// Aura authorities
		(
			hex!("57f8dc2f5ab09467896f47300f0424385e0621c4869aa60c02be9adcc98a0d1d").into(),
			hex!("04eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116").into()
		),
		// AuraExt authorities
		(
			hex!("3c311d57d4daf52904616cf69648081e5e0621c4869aa60c02be9adcc98a0d1d").into(),
			hex!("04eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116").into()
		),
		// parachainSystem lastDmqMqcHead (emtpy)
		(
			hex!("45323df7cc47150b3930e2666b0aa313911a5dd3f1155f5b7d0c5aa102a757f9").into(),
			hex!("0000000000000000000000000000000000000000000000000000000000000000").into()
		)
		])
	};

	overrides
}

fn calculate_genesis_slot(current: Vec<u8>, epoch_idx: Vec<u8>) -> Slot {
	const EPOCH_DURARION: u64 = 2400;
	const DIFF_TARGET: u64 = 2390;
	let current_slot: Slot = Decode::decode(&mut current.as_slice()).unwrap();
	let epoch_index: u64 = Decode::decode(&mut epoch_idx.as_slice()).unwrap();
	let genesis_slot = current_slot.saturating_sub((epoch_index * EPOCH_DURARION) + DIFF_TARGET);
	genesis_slot
}

#[cfg(test)]
mod tests {
	use super::calculate_genesis_slot;
    use codec::{Encode, Decode};
    use polkadot_primitives::Slot;

	#[test]
	#[ignore]
	fn calculate_slot() {
		const EPOCH_DURARION: u64 = 2400;
		let current_slot: Slot = Decode::decode(&mut hex::decode("daf0341100000000").unwrap().as_slice()).unwrap();
		let epoch_index: u64 = Decode::decode(&mut hex::decode("6826000000000000").unwrap().as_slice()).unwrap();
		println!("{current_slot}");

		let slots = EPOCH_DURARION * epoch_index;

		let genesis = calculate_genesis_slot(hex::decode("daf0341100000000").unwrap(), hex::decode("6826000000000000").unwrap());

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
		let s: polkadot_primitives::SessionInfo = codec::Decode::decode(&mut data.as_slice()).unwrap();
		println!("{:#?}", s);

		let host_config_value_hex = "0000300000500000aaaa020000001000fbff0000100000000a000000403800005802000003000000020000000000500000c800008000000000e8764817000000000000000000000000e87648170000000000000000000000e80300000090010080000000009001000c01002000000600c4090000000000000601983a000000000000403800000006000000580200000300000059000000000000001e000000060000000200000014000000020000000803060000000a0000000a0000000105000000020000003e00000000000000f401000080b2e60e80c3c90180b2e60e00000000000000000000000005000000";
		let data = hex::decode(host_config_value_hex).unwrap();
		let mut host_config: HostConfiguration<BlockNumber> = codec::Decode::decode(&mut data.as_slice()).unwrap();
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