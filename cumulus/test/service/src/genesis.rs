// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use codec::Encode;
use cumulus_primitives_core::ParaId;
use cumulus_test_runtime::Block;
use polkadot_primitives::HeadData;
use sc_chain_spec::ChainSpec;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header as HeaderT, Zero},
	StateVersion,
};

/// Generate a simple test genesis block from a given ChainSpec.
pub fn generate_genesis_block<Block: BlockT>(
	chain_spec: &dyn ChainSpec,
	genesis_state_version: StateVersion,
) -> Result<Block, String> {
	let storage = chain_spec.build_storage()?;

	let child_roots = storage.children_default.iter().map(|(sk, child_content)| {
		let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
			child_content.data.clone().into_iter().collect(),
			genesis_state_version,
		);
		(sk.clone(), state_root.encode())
	});
	let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
		storage.top.clone().into_iter().chain(child_roots).collect(),
		genesis_state_version,
	);

	let extrinsics_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
		Vec::new(),
		genesis_state_version,
	);

	Ok(Block::new(
		<<Block as BlockT>::Header as HeaderT>::new(
			Zero::zero(),
			extrinsics_root,
			state_root,
			Default::default(),
			Default::default(),
		),
		Default::default(),
	))
}

/// Returns the initial head data for a parachain ID.
pub fn initial_head_data(para_id: ParaId) -> HeadData {
	let spec = crate::chain_spec::get_chain_spec(Some(para_id));
	let block: Block = generate_genesis_block(&spec, sp_runtime::StateVersion::V1).unwrap();
	let genesis_state = block.header().encode();
	genesis_state.into()
}
