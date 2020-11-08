// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! A Cumulus test client.

mod block_builder;

pub use block_builder::*;

use codec::Encode;
pub use cumulus_test_runtime as runtime;
use runtime::{
	Balance, Block, BlockHashCount, Call, GenesisConfig, Runtime, Signature, SignedExtra,
	SignedPayload, UncheckedExtrinsic, VERSION,
};
use sc_service::client;
use sp_blockchain::HeaderBackend;
use sp_core::{map, storage::Storage, twox_128, ChangesTrieConfiguration};
use sp_runtime::{
	generic::Era,
	traits::{Block as BlockT, Hash as HashT, Header as HeaderT},
	BuildStorage, SaturatedConversion,
};
use std::collections::BTreeMap;
pub use substrate_test_client::*;

mod local_executor {
	use substrate_test_client::sc_executor::native_executor_instance;
	native_executor_instance!(
		pub LocalExecutor,
		cumulus_test_runtime::api::dispatch,
		cumulus_test_runtime::native_version,
	);
}

/// Native executor used for tests.
pub use local_executor::LocalExecutor;

/// Test client database backend.
pub type Backend = substrate_test_client::Backend<Block>;

/// Test client executor.
pub type Executor = client::LocalCallExecutor<Backend, sc_executor::NativeExecutor<LocalExecutor>>;

/// Test client builder for Cumulus
pub type TestClientBuilder =
	substrate_test_client::TestClientBuilder<Block, Executor, Backend, GenesisParameters>;

/// LongestChain type for the test runtime/client.
pub type LongestChain = sc_consensus::LongestChain<Backend, Block>;

/// Test client type with `LocalExecutor` and generic Backend.
pub type Client = client::Client<Backend, Executor, Block, runtime::RuntimeApi>;

/// Parameters of test-client builder with test-runtime.
#[derive(Default)]
pub struct GenesisParameters {
	support_changes_trie: bool,
}

impl substrate_test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> Storage {
		let changes_trie_config: Option<ChangesTrieConfiguration> = if self.support_changes_trie {
			Some(sp_test_primitives::changes_trie_config())
		} else {
			None
		};
		let mut storage = genesis_config(changes_trie_config).build_storage().unwrap();

		let child_roots = storage.children_default.iter().map(|(sk, child_content)| {
			let state_root =
				<<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
					child_content.data.clone().into_iter().collect(),
				);
			(sk.clone(), state_root.encode())
		});
		let state_root =
			<<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
				storage.top.clone().into_iter().chain(child_roots).collect(),
			);
		let block: runtime::Block = client::genesis::construct_genesis_block(state_root);
		storage.top.extend(additional_storage_with_genesis(&block));

		storage
	}
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt: Sized {
	/// Enable or disable support for changes trie in genesis.
	fn set_support_changes_trie(self, support_changes_trie: bool) -> Self;

	/// Build the test client.
	fn build(self) -> Client {
		self.build_with_longest_chain().0
	}

	/// Build the test client and longest chain selector.
	fn build_with_longest_chain(self) -> (Client, LongestChain);
}

impl TestClientBuilderExt for TestClientBuilder {
	fn set_support_changes_trie(mut self, support_changes_trie: bool) -> Self {
		self.genesis_init_mut().support_changes_trie = support_changes_trie;
		self
	}

	fn build_with_longest_chain(self) -> (Client, LongestChain) {
		self.build_with_native_executor(None)
	}
}

/// A `TestClientBuilder` with default backend and executor.
pub trait DefaultTestClientBuilderExt: Sized {
	/// Create new `TestClientBuilder`
	fn new() -> Self;
}

impl DefaultTestClientBuilderExt for TestClientBuilder {
	fn new() -> Self {
		Self::with_default_backend()
	}
}

fn genesis_config(changes_trie_config: Option<ChangesTrieConfiguration>) -> GenesisConfig {
	cumulus_test_service::local_testnet_genesis(changes_trie_config)
}

fn additional_storage_with_genesis(genesis_block: &Block) -> BTreeMap<Vec<u8>, Vec<u8>> {
	map![
		twox_128(&b"latest"[..]).to_vec() => genesis_block.hash().as_fixed_bytes().to_vec()
	]
}

/// Generate an extrinsic from the provided function call, origin and [`Client`].
pub fn generate_extrinsic(
	client: &Client,
	origin: sp_keyring::AccountKeyring,
	function: Call,
) -> UncheckedExtrinsic {
	let current_block_hash = client.info().best_hash;
	let current_block = client.info().best_number.saturated_into();
	let genesis_block = client.hash(0).unwrap().unwrap();
	let nonce = 0;
	let period = BlockHashCount::get()
		.checked_next_power_of_two()
		.map(|c| c / 2)
		.unwrap_or(2) as u64;
	let tip = 0;
	let extra: SignedExtra = (
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckEra::<Runtime>::from(Era::mortal(period, current_block)),
		frame_system::CheckNonce::<Runtime>::from(nonce),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
	);
	let raw_payload = SignedPayload::from_raw(
		function.clone(),
		extra.clone(),
		(
			VERSION.spec_version,
			genesis_block,
			current_block_hash,
			(),
			(),
			(),
		),
	);
	let signature = raw_payload.using_encoded(|e| origin.sign(e));

	UncheckedExtrinsic::new_signed(
		function.clone(),
		origin.public().into(),
		Signature::Sr25519(signature.clone()),
		extra.clone(),
	)
}

/// Transfer some token from one account to another using a provided test [`Client`].
pub fn transfer(
	client: &Client,
	origin: sp_keyring::AccountKeyring,
	dest: sp_keyring::AccountKeyring,
	value: Balance,
) -> UncheckedExtrinsic {
	let function = Call::Balances(pallet_balances::Call::transfer(dest.public().into(), value));

	generate_extrinsic(client, origin, function)
}
