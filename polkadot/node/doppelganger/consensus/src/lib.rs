use std::collections::HashMap;

use hex_literal::hex;
use log::debug;
use sc_consensus::{
	block_import::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult},
	StateAction,
};
use sp_runtime::traits::{Block as BlockT, HashingFor, Header, PhantomData};
use sp_trie::{trie_types::TrieDBMutBuilderV1, MemoryDB, TrieMut};

const LOG_TARGET: &str = "doppelganger";

pub struct DoppelGangerBlockImport<BI, Block>
where
	Block: BlockT,
	BI: BlockImport<Block>,
{
	inner: BI,
	_phantom: PhantomData<Block>,
}

impl<Block: BlockT, BI: BlockImport<Block>> DoppelGangerBlockImport<BI, Block> {
	pub fn new(inner: BI) -> Self {
		println!("Wrapping with DoppelGangerBlockImport");
		DoppelGangerBlockImport { inner, _phantom: PhantomData }
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
		let number = *block.header.number();
		if block.with_state() {
			debug!(target: LOG_TARGET, "Block param with state, with header {:?}", block.header);
			if let StateAction::ApplyChanges(sc_consensus::StorageChanges::Import(
				ref mut imported_state,
			)) = block.state_action
			{
				let mut mdb = MemoryDB::default();
				let mut state_root = Block::Hash::default();

				let overrides = get_overrides();
				{
					let mut trie =
						TrieDBMutBuilderV1::<HashingFor<Block>>::new(&mut mdb, &mut state_root)
							.build();

					for key_value_storage_level in imported_state.state.0.iter_mut() {
						println!("state_root: {:?}", key_value_storage_level.state_root);
						println!(
							"parent_storage_keys: {:?}",
							key_value_storage_level.parent_storage_keys
						);

						for kv in key_value_storage_level.key_values.iter_mut() {
							if std::env::var("ZOMBIE_DUMP").is_ok() {
								eprintln!("{}: {}", sp_core::hexdisplay::HexDisplay::from(&kv.0), sp_core::hexdisplay::HexDisplay::from(&kv.1));
							}

							if let Some(override_value) = overrides.get(&kv.0) {
								println!(
									"Overriding key: {}",
									sp_core::hexdisplay::HexDisplay::from(&kv.0)
								);
								println!(
									"old value: {}",
									sp_core::hexdisplay::HexDisplay::from(&kv.1)
								);
								kv.1 = override_value.clone();
								println!(
									"new value: {}",
									sp_core::hexdisplay::HexDisplay::from(&kv.1)
								);
							}

							trie.insert(&kv.0, &kv.1).expect("TrieMut::insert should not fail");
						}
					}
				}

				block.header.set_state_root(state_root);
				block.post_hash = Some(block.post_header().hash());
				// NOT create gap
				block.create_gap = false;
			}
			let res = self.inner.import_block(block).await;
			println!("Block import done! : {:?}, killing the process", res);
			// use last line to share the block number
			println!("{}", number);
			std::process::exit(0);
		}

		return self.inner.import_block(block).await
	}
}

// Default keys used for Alice/Bob
fn get_overrides() -> HashMap<Vec<u8>, Vec<u8>> {
	let overrides: HashMap<Vec<u8>, Vec<u8>> = HashMap::from([
		// <Pallet> < Item>
		// Validator Validators
		(
			hex!("7d9fe37370ac390779f35763d98106e888dcde934c658227ee1dfafcd6e16903").into(),
			hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e").into()
		),
		// Session Validators
		(
			hex!("cec5070d609dd3497f72bde07fc96ba088dcde934c658227ee1dfafcd6e16903").into(),
			hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e").into()
		),
		//	Session QueuedKeys
		(
			hex!("cec5070d609dd3497f72bde07fc96ba0e0cdd062e6eaf24295ad4ccfc41d4609").into(),
			hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0eed43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1fe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860ed17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae698eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27").into()
		),
		// Babe Authorities
		(
			hex!("1cb6f36e027abb2091cfb5110ab5087f5e0621c4869aa60c02be9adcc98a0d1d").into(),
			hex!("08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d01000000000000008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480100000000000000").into()
		),
		// Babe NextAuthorities
		(
			hex!("1cb6f36e027abb2091cfb5110ab5087faacf00b9b41fda7a9268821c2a2b3e4c").into(),
			hex!("08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d01000000000000008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480100000000000000").into()
		),
		// Grandpa Authorities
		(
			hex!("5f9cc45b7a00c5899361e1c6099678dc5e0621c4869aa60c02be9adcc98a0d1d").into(),
			hex!("0888dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee0100000000000000d17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae690100000000000000").into()
		),
		// Staking ForceEra
		(
			hex!("5f3e4907f716ac89b6347d15ececedcaf7dad0317324aecae8744b87fc95f2f3").into(),
			hex!("02").into()
		),

		// Staking Invulnerables
		(
			hex!("5f3e4907f716ac89b6347d15ececedca5579297f4dfb9609e7e4c2ebab9ce40a").into(),
			hex!("08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e").into()
		),

		// System Accounts?
	]);

	overrides
}
