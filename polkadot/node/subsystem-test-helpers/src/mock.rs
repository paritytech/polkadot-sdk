// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use std::sync::Arc;

use polkadot_node_subsystem::{jaeger, ActivatedLeaf, BlockInfo};
use sc_client_api::UnpinHandle;
use sc_keystore::LocalKeystore;
use sc_utils::mpsc::tracing_unbounded;
use sp_application_crypto::AppCrypto;
use sp_keyring::Sr25519Keyring;
use sp_keystore::{Keystore, KeystorePtr};

use polkadot_primitives::{AuthorityDiscoveryId, Block, BlockNumber, Hash, ValidatorId};

/// Get mock keystore with `Ferdie` key.
pub fn make_ferdie_keystore() -> KeystorePtr {
	let keystore: KeystorePtr = Arc::new(LocalKeystore::in_memory());
	Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Ferdie.to_seed()),
	)
	.expect("Insert key into keystore");
	Keystore::sr25519_generate_new(
		&*keystore,
		AuthorityDiscoveryId::ID,
		Some(&Sr25519Keyring::Ferdie.to_seed()),
	)
	.expect("Insert key into keystore");
	keystore
}

/// Create a meaningless unpin handle for a block.
pub fn dummy_unpin_handle(block: Hash) -> UnpinHandle<Block> {
	let (dummy_sink, _) = tracing_unbounded("Expect Chaos", 69);
	UnpinHandle::new(block, dummy_sink)
}

/// Create a new leaf with the given hash and number.
pub fn new_leaf(hash: Hash, number: BlockNumber) -> ActivatedLeaf {
	ActivatedLeaf {
		hash,
		number,
		unpin_handle: dummy_unpin_handle(hash),
		span: Arc::new(jaeger::Span::Disabled),
	}
}

/// Create a new leaf with the given hash and number.
pub fn new_block_import_info(hash: Hash, number: BlockNumber) -> BlockInfo {
	BlockInfo { hash, parent_hash: Hash::default(), number, unpin_handle: dummy_unpin_handle(hash) }
}
